use std::{
    env,
    io::Write,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use actix_files::NamedFile;
use actix_web::{
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};

use clap::Parser;
use fern::colors::{Color, ColoredLevelConfig};
use partials::FooterArgs;

mod partials;

lazy_static::lazy_static! {
    static ref PWD: Arc<RwLock<PathBuf>> = Arc::new(RwLock::new(env::current_dir().unwrap()));
}

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[clap(index = 1)]
    dir: Option<PathBuf>,

    #[clap(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    #[clap(short, long, default_value = "8080")]
    port: u16,

    #[clap(short, long, default_value_t = cfg!(debug_assertions), help="Don't open in browser")]
    no_open: bool,

    #[clap(short, long, default_value_t = false, conflicts_with = "trace")]
    verbose: bool,

    #[clap(long, default_value_t = false, conflicts_with = "verbose")]
    trace: bool,

    #[clap(
        short,
        long,
        default_value_t = false,
        help = "Allow directory traversal"
    )]
    traverse: bool,
}

pub fn setup_logging(loglevel: log::LevelFilter, to_file: bool) -> Result<(), log::SetLoggerError> {
    let colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .warn(Color::Yellow)
        .error(Color::Red)
        .debug(Color::Cyan)
        .trace(Color::Magenta);

    // setup a stdio logger
    let stdio_log = fern::Dispatch::new()
        .level(loglevel)
        .format({
            let colors = colors.clone();
            move |out, message, record| {
                out.finish(format_args!(
                    "[{:^7}] [{}] {}",
                    colors.color(record.level()),
                    record.target(),
                    message
                ))
            }
        })
        .chain(std::io::stdout());

    // and a file logger
    if to_file {
        let file_log = fern::Dispatch::new()
            .level(loglevel)
            .format({
                move |out, message, record| {
                    out.finish(format_args!(
                        "{} [{}] [{}] {}",
                        chrono::Local::now().format("%+"),
                        record.level(),
                        record.target(),
                        message
                    ))
                }
            })
            .chain(fern::Output::call({
                // this is cursed (maybe, idk, it works for now)
                // poor girls async file logger, with daily rotation, and a 10 second sync interval
                // uses an mpsc channel to send everything to a background thread, which then writes to the files
                // this does not ensure that log rows are in order, but it does stop the output from being garbled

                let get_current_filename = || {
                    let date = chrono::Local::now().format("%Y-%m-%d-UTC%Z").to_string();
                    format!("logs/iv-{}.log", date)
                };

                let (tx, rx): (
                    std::sync::mpsc::Sender<(String, String)>,
                    std::sync::mpsc::Receiver<(String, String)>,
                ) = std::sync::mpsc::channel();

                let mut open_files: std::collections::HashMap<String, std::fs::File> =
                    Default::default();

                let filename = get_current_filename();

                if !PathBuf::from("log").exists() {
                    std::fs::create_dir("log").unwrap();
                }

                open_files.insert(
                    filename.clone(),
                    std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(filename)
                        .unwrap(),
                );

                let mut last_flush = std::time::Instant::now();

                std::thread::spawn({
                    let mut current_filename = get_current_filename();

                    move || {
                        for (filename, message) in rx {
                            // get current file, and open new one if needed
                            let file = open_files.entry(filename.clone()).or_insert_with(|| {
                                std::fs::OpenOptions::new()
                                    .append(true)
                                    .create(true)
                                    .open(filename)
                                    .unwrap()
                            });

                            writeln!(file, "{}", message).unwrap();

                            if last_flush.elapsed().as_secs() > 10 {
                                for file in open_files.values() {
                                    file.sync_all().unwrap_or(());
                                }
                                last_flush = std::time::Instant::now();

                                // close last file if dates changed
                                // there may be some fucked up edgecase where a thread thinks its still
                                // yesterday and it opens the file again, but should work out next time this runs
                                if current_filename != get_current_filename() {
                                    open_files.remove(&current_filename).map(|file| {
                                        file.sync_all().unwrap();
                                    });
                                    // (file handle gets dropped after .remove.map, and subsequently closed)

                                    current_filename = get_current_filename();
                                }
                            }
                        }
                    }
                });

                // actual lambda passed to fern::Output::call
                move |record| {
                    let fmt = format!("{}", record.args());
                    tx.send((get_current_filename(), fmt)).unwrap_or(())
                }
            }));

        // combine the two loggers
        fern::Dispatch::new()
            .chain(stdio_log)
            .chain(file_log)
            .apply()
    } else {
        fern::Dispatch::new().chain(stdio_log).apply()
    }
}

fn visit_dir(dir: &PathBuf) -> Vec<PathBuf> {
    dir.read_dir()
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>()
}

fn stat_all(dirs: Vec<PathBuf>) -> Vec<(PathBuf, std::fs::Metadata)> {
    dirs.into_iter()
        .map(|path| (path.clone(), path.metadata()))
        .filter(|(_, meta)| meta.is_ok())
        .map(|(path, meta)| (path, meta.unwrap()))
        .collect()
}

// Canonicalizes the path and returns it if its allowed
fn canonicalize_path(
    path: &PathBuf,
    args: &Args,
    pwd: &PathBuf,
    allow_nondir: bool,
) -> Option<PathBuf> {
    let base_dir = pwd.canonicalize().unwrap();

    let mut target_path = base_dir.clone();

    if path.is_absolute() {
        target_path.push(path.strip_prefix("/").unwrap_or(&path));
    } else {
        target_path.push(path);
    }

    if !target_path.exists() {
        return None;
    }

    let mut target_path = target_path.canonicalize().unwrap_or(target_path);

    if !allow_nondir && !target_path.is_dir() {
        target_path.pop();
    }

    if !args.traverse && target_path != base_dir {
        return None;
    }

    if target_path.starts_with(base_dir) {
        return Some(target_path);
    } else {
        return None;
    }
}

async fn index(
    req: HttpRequest,
    args: web::Data<Args>,
    pwd: web::Data<Arc<RwLock<PathBuf>>>,
) -> impl Responder {
    let path = PathBuf::from(String::from(urlencoding::decode(req.path()).unwrap()));

    if let Some(path) = canonicalize_path(&path, &args, &PWD.read().unwrap(), false) {
        let mut dirs = stat_all(visit_dir(&path));

        dirs.sort_by_cached_key(|(path, _)| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase()
                .to_string()
        });
        dirs.sort_by_key(|(_, meta)| !meta.is_dir());

        let num_dirs = dirs.iter().fold(
            0,
            |acc, (path, _)| if path.is_dir() { acc + 1 } else { acc },
        );

        let total_size = dirs.iter().fold(
            0,
            |acc, (_, meta)| if meta.is_dir() { acc } else { acc + meta.len() },
        );

        let mut pwd = pwd.read().unwrap().to_string_lossy().to_string();
        let home = env::var("HOME").unwrap_or("/".to_string());

        if pwd.starts_with(&home) {
            pwd = pwd.replace(&home, "~");
        }

        return HttpResponse::Ok().body(
            partials::page(
                "iv",
                &pwd,
                &path,
                FooterArgs {
                    num_entries: dirs.len(),
                    num_dirs,
                    total_size,
                },
                partials::entry_grid(&args, dirs),
            )
            .into_string(),
        );
    }

    return HttpResponse::TemporaryRedirect()
        .append_header(("Location", "/"))
        .finish();
}

async fn assets(req: HttpRequest, args: web::Data<Args>) -> actix_web::Result<NamedFile> {
    let path = PathBuf::from(String::from(urlencoding::decode(req.path()).unwrap()));
    let path = PathBuf::from("assets").join(path.strip_prefix("/_!").unwrap_or(&path));

    if let Some(path) = canonicalize_path(&path, &args, &env::current_dir().unwrap(), true) {
        Ok(NamedFile::open(path)?)
    } else {
        Err(actix_web::error::ErrorNotFound("404 Not Found"))
    }
}

async fn file(req: HttpRequest, args: web::Data<Args>) -> actix_web::Result<NamedFile> {
    let path = PathBuf::from(String::from(urlencoding::decode(req.path()).unwrap()));
    let path = PathBuf::from(path.strip_prefix("/!_").unwrap_or(&path));

    if path.is_dir() {
        return Err(actix_web::error::ErrorNotFound("404 Not Found"));
    }

    if let Some(path) = canonicalize_path(&path, &args, &PWD.read().unwrap(), true) {
        Ok(NamedFile::open(path)?)
    } else {
        Err(actix_web::error::ErrorNotFound("404 Not Found"))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("iv~~!");

    let args = Args::parse();

    if args.dir.is_some() {
        *PWD.write().unwrap() = args.dir.clone().unwrap();
    }

    setup_logging(
        if args.trace {
            log::LevelFilter::Trace
        } else if cfg!(debug_assertions) || args.verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        },
        false,
    )
    .unwrap();

    log::debug!("args: {:?}", args);
    log::debug!("pwd: {:?}", env::current_dir().unwrap());

    if !args.no_open {
        std::thread::spawn({
            let args = args.clone();

            move || {
                std::thread::sleep(std::time::Duration::from_millis(500));

                opener::open_browser(format!("http://{}:{}", args.host, args.port)).unwrap();
            }
        });
    }

    HttpServer::new({
        let args = args.clone();
        move || {
            App::new()
                .app_data(Data::new(args.clone()))
                .app_data(Data::new(PWD.clone()))
                .default_service(web::route().to(index))
                // FUCK me if someone uses _! to prefix a filename
                .service(web::resource("/_!/{path:.*}").to(assets))
                .service(web::resource("/!_/{path:.*}").to(file))
        }
    })
    .bind((args.host, args.port))?
    .run()
    .await
}
