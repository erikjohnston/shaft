#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate rusqlite;

use daemonize::Daemonize;
use sloggers::Config;

use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

mod db;
mod github;
mod rest;

type HttpClient = hyper::Client<hyper_tls::HttpsConnector<hyper::client::HttpConnector>>;

macro_rules! load_template {
    ($logger:expr, $hb:expr, $root:expr, $name:expr) => {
        if let Err(e) = load_template_impl(&mut $hb, $root, $name) {
            crit!($logger, "Failed to load resources/{}.hbs: {}", $name, e);
            return;
        }
    };
}

#[derive(Debug, Deserialize)]
struct GithubSettings {
    client_id: String,
    client_secret: String,
    state: String,
    required_org: String,
}

#[derive(Debug, Deserialize)]
struct DaemonizeSettings {
    pid_file: String,
}

#[derive(Debug, Deserialize)]
struct Settings {
    github: GithubSettings,
    database_file: String,
    resource_dir: String,
    web_root: String,
    bind: String,
    log: sloggers::LoggerConfig,
    daemonize: Option<DaemonizeSettings>,
}

fn main() {
    let mut c = config::Config::new();
    c.merge(config::File::with_name("settings.toml")).unwrap();

    let settings: Settings = c.try_into().unwrap();

    let logger = settings.log.build_logger().unwrap();

    let addr = &settings.bind;

    let mut hb = handlebars::Handlebars::new();

    load_template!(logger, hb, &settings.resource_dir, "index");
    load_template!(logger, hb, &settings.resource_dir, "login");
    load_template!(logger, hb, &settings.resource_dir, "transactions");
    load_template!(logger, hb, &settings.resource_dir, "base");

    if let Some(daemonize_settings) = settings.daemonize {
        Daemonize::new()
            .pid_file(daemonize_settings.pid_file)
            .start()
            .expect("be able to daemonize");
    }

    hb.register_helper(
        "pence-as-pounds",
        Box::new(rest::format_pence_as_pounds_helper),
    );

    let github_client_id = settings.github.client_id.clone();
    let github_client_secret = settings.github.client_secret.clone();
    let github_state = settings.github.state.clone();

    let database = Arc::new(db::SqliteDatabase::with_path(settings.database_file));
    let app_config = rest::AppConfig {
        github_client_id,
        github_client_secret,
        github_state,
        web_root: settings.web_root.clone(),
        required_org: settings.github.required_org.clone(),
        resource_dir: settings.resource_dir.clone(),
    };
    let cpu_pool = futures_cpupool::CpuPool::new(40);

    let https = hyper_tls::HttpsConnector::new(4).expect("TLS initialization failed");
    let http_client = hyper::Client::builder().build::<_, hyper::Body>(https);

    let app_state = rest::AppState {
        database,
        config: app_config,
        cpu_pool,
        handlebars: Arc::new(hb),
        http_client,
    };

    // let mut server = Server::with_logger(logger.clone());

    // Set up all the state for the server to manage, e.g. database,
    // http client, etc
    // let db: Rc<db::Database> = Rc::new(db::SqliteDatabase::with_path(settings.database_file));
    // server.manage_state(db);

    // Start up the server ...
    actix_web::server::HttpServer::new(move || {
        let app = actix_web::App::with_state(app_state.clone());
        let app = app.middleware(rest::MiddlewareLogger::new(logger.clone()));

        // Now actually register the various servlets
        rest::register_servlets(app)
    })
    .bind(addr)
    .unwrap()
    .start();
}

fn load_template_impl(
    hb: &mut handlebars::Handlebars,
    root: &str,
    name: &str,
) -> Result<(), Box<Error>> {
    let mut index_file = File::open(format!("{}/{}.hbs", root, name))?;
    let mut source = String::new();
    index_file.read_to_string(&mut source)?;
    hb.register_template_string(name, source)?;

    Ok(())
}
