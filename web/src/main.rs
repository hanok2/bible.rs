#![warn(clippy::all)]

use std::env;
use std::error::Error;
use std::io;

use actix_files;
use actix_web::{http::ContentEncoding, middleware, web, App, HttpServer};
use dotenv::dotenv;
use handlebars::Handlebars;

use db::{build_pool, establish_connection, run_migrations, SqliteConnectionPool, SwordDrill};
use sentry_actix::SentryMiddleware;

use crate::controllers::{api, view};

/// Represents the [server data](actix_web.web.Data.html) for the application.
pub struct ServerData {
    pub db: SqliteConnectionPool,
    pub template: Handlebars,
}

/// Registers the [Handlebars](handlebars.handlebars.html) templates for the application.
fn register_templates() -> Result<Handlebars, Box<dyn Error>> {
    let mut tpl = Handlebars::new();
    tpl.set_strict_mode(true);
    tpl.register_templates_directory(".hbs", "./web/templates/")?;

    Ok(tpl)
}

fn main() -> io::Result<()> {
    dotenv().ok();

    // Set up logging
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Get env configuration
    let capture_errors = env::var("CAPTURE_ERRORS")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .expect("Invalid value for CAPTURE_ERRORS. Must be 'true' or 'false.'");
    let url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Set up sentry
    let _guard = sentry::init("https://b2dc20aad7f64ae9a9c5e8a77e947d9c@sentry.io/1768339");
    if capture_errors {
        sentry::integrations::panic::register_panic_handler();
    }

    // Run DB migrations for a new SQLite database
    run_migrations(&establish_connection(&url)).expect("Error running migrations");

    let pool = build_pool(&url);

    HttpServer::new(move || {
        // Create handlebars registry
        let template = register_templates().unwrap();

        // Wire up the application
        App::new()
            .wrap(middleware::Compress::new(ContentEncoding::Gzip))
            .wrap(
                SentryMiddleware::new()
                    .emit_header(true)
                    .capture_server_errors(capture_errors),
            )
            .wrap(middleware::Logger::default())
            .data(ServerData {
                db: pool.clone(),
                template,
            })
            .service(actix_files::Files::new("/static", "./web/dist").use_etag(true))
            .service(web::resource("about").to(view::about))
            .service(
                web::resource("/")
                    .name("bible")
                    .route(web::get().to_async(view::all_books::<SwordDrill>)),
            )
            .service(web::resource("search").route(web::get().to_async(view::search::<SwordDrill>)))
            .service(
                web::resource("{book}")
                    .name("book")
                    .route(web::get().to_async(view::book::<SwordDrill>)),
            )
            .service(
                web::resource("{reference:.+\\d}")
                    .name("reference")
                    .route(web::get().to_async(view::reference::<SwordDrill>)),
            )
            .service(
                web::resource("api/search").route(web::get().to_async(api::search::<SwordDrill>)),
            )
            .service(
                web::resource("api/{reference}.json")
                    .route(web::get().to_async(api::reference::<SwordDrill>)),
            )
            .default_service(web::route().to(web::HttpResponse::NotFound))
    })
    .bind("0.0.0.0:8080")?
    .run()
}

mod controllers;
mod error;
mod macros;
mod responder;
#[cfg(test)]
mod test;
