extern crate gleam;
#[macro_use]
extern crate gleam_derive;
extern crate hyper;
extern crate futures_cpupool;
extern crate toml;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate serde_urlencoded;
extern crate regex;
extern crate r2d2;
extern crate r2d2_sqlite;
extern crate futures;
#[macro_use]
extern crate quick_error;
extern crate rusqlite;
extern crate url;
extern crate tokio_core;
extern crate hyper_tls;
extern crate rand;
extern crate chrono;
extern crate anymap;
extern crate handlebars;
extern crate itertools;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;
extern crate linear_map;
extern crate glob;
extern crate mime_guess;
extern crate config;
extern crate sloggers;
extern crate daemonize;


use daemonize::{Daemonize};
use gleam::Server;
use futures::Stream;
use sloggers::Config;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;

use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::rc::Rc;
use std::net;


mod db;
mod github;
mod rest;


type HttpClient = Rc<hyper::Client<hyper_tls::HttpsConnector<hyper::client::HttpConnector>>>;


macro_rules! load_template {
    ($logger:expr, $hb:expr, $root:expr, $name:expr) => (
        match load_template_impl(&mut $hb, $root, $name) {
            Err(e) => {
                crit!($logger, "Failed to load resources/{}.hbs: {}", $name, e);
                return
            },
            _ => {}
        }
    )
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
    let settings: Settings = config::Config::default()
        .merge(config::File::with_name("settings.toml")).unwrap()
        .deserialize().unwrap();

    let logger = settings.log.build_logger().unwrap();

    let addr = String::from(settings.bind).parse().unwrap();
    let blocking_listener = net::TcpListener::bind(&addr).unwrap();

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

    let mut server = Server::with_logger(logger.clone());

    // Set up all the state for the server to manage, e.g. database,
    // http client, etc
    let db: Rc<db::Database> = Rc::new(
        db::SqliteDatabase::with_path(settings.database_file)
    );
    server.manage_state(db);

    let github_client_id = settings.github.client_id.clone();
    let github_client_secret = settings.github.client_secret.clone();
    let github_state = settings.github.state.clone();

    server.manage_state(rest::AppConfig {
        github_client_id,
        github_client_secret,
        github_state,
        web_root: settings.web_root.clone(),
        required_org: settings.github.required_org.clone(),
        resource_dir: settings.resource_dir.clone(),
    });

    hb.register_helper("pence-as-pounds", Box::new(rest::format_pence_as_pounds_helper));

    server.manage_state(Rc::new(hb));

    let cpu_pool = futures_cpupool::CpuPool::new(40);
    server.manage_state(cpu_pool);

    // Now actually register the various servlets
    rest::register_servlets(&mut server);

    // Set up tokio reactor
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let client = hyper::Client::configure()
        .connector(hyper_tls::HttpsConnector::new(4, &handle).unwrap())
        .build(&handle);

    server.manage_state(Rc::new(client));

    // Start up the server ...
    let listener = TcpListener::from_listener(blocking_listener, &addr, &handle).unwrap();
    let protocol = hyper::server::Http::new();

    let server_arc = Rc::new(server);
    let srv = listener.incoming().for_each(|(socket, addr)| {
        protocol.bind_connection(&handle, socket, addr, server_arc.clone());
        Ok(())
    });

    core.run(srv).unwrap();
}


fn load_template_impl(
    hb: &mut handlebars::Handlebars,
    root: &str,
    name: &str,
)
    -> Result<(), Box<Error>>
{
    let mut index_file = File::open(
        format!("{}/{}.hbs", root, name)
    )?;
    let mut source = String::new();
    index_file.read_to_string(&mut source)?;
    hb.register_template_string(name, source)?;

    Ok(())
}
