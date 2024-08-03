use std::{fs::Metadata, os::linux::fs::MetadataExt, path::PathBuf};

use maud::{html, Markup, DOCTYPE};

use crate::{Args, PWD};

pub fn header(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        link rel="stylesheet" href="/_!/reset.css";
        link rel="stylesheet" href="/_!/iv.css";
        link rel="icon" type="image/png" href="/_!/favicon.png";

        head {
            title { (page_title) }
        }
    }
}

pub static VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct FooterArgs {
    pub num_entries: usize,
    pub num_dirs: usize,
    pub total_size: u64,
}

pub fn footer(args: FooterArgs) -> Markup {
    let sizes = ["B", "KB", "MB", "GB", "TB"];

    let mut size = args.total_size as f64;
    let mut i = 0;
    while size >= 1024.0 && i < sizes.len() {
        size /= 1024.0;
        i += 1;
    }

    let size_str = {
        if i == 0 {
            format!("{} {}", size as u64, sizes[i])
        } else {
            format!("{:.2} {}", size, sizes[i])
        }
    };

    html! {
        footer {
            div class="info" {
                p class="entries" {
                    (args.num_entries)
                    " entries (";
                    (args.num_dirs)
                    " dir";
                    (
                        if args.num_dirs == 1 { "), " } else { "s), " }
                    )
                    (size_str)
                }
                p class="version" {
                    (format!("iv~~ v{}", VERSION))
                }
            }
        }
    }
}

pub fn breadcrumb(uri_path: &str, path: &PathBuf) -> Markup {
    let pwd = PWD.read().unwrap();

    let rel_path = path
        .strip_prefix(pwd.clone())
        .unwrap_or(path)
        .as_os_str()
        .to_string_lossy()
        .to_string();

    let parts = std::iter::once(uri_path.to_string())
        .chain(rel_path.split('/').map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut hrefs = vec![String::from("/")];
    for i in 1..parts.len() {
        hrefs.push(format!("/{}", parts[1..=i].join("/")));
    }

    html! {
        div class="breadcrumb" {
            @for (i, part) in parts.iter().enumerate() {
                @if i == 0 {
                    a href=(hrefs[i]) { (part) }
                } @else {
                    span class="sep" { " / " }
                    a href=(hrefs[i]) { (part) }
                }
            }
        }
    }
}

pub fn page(
    page_title: &str,
    uri_path: &str,
    path: &PathBuf,
    footer_args: FooterArgs,
    content: Markup,
) -> Markup {
    html! {
        (header(format!("{} | {}", page_title, uri_path).as_str()))
        body {
            div class="container" {
                header class="header" {
                    h1 { (page_title) }
                    div class="vr" {}
                    (breadcrumb(uri_path, path));
                }
                div class="content" {
                    (content)
                }
                (footer(footer_args))
            }
        }
    }
}

pub fn icon(name: &str, size: usize) -> Markup {
    html! {
        i
        class="material-icons"
        style=(format!("font-size: {}px;", size))
        { (name) }
    }
}

pub fn file_hash_id(meta: &Metadata) -> String {
    let id = xorshift64(
        (meta
            .st_atime()
            .wrapping_add(meta.st_mtime())
            .wrapping_add(meta.st_ctime())) as u64,
    );

    let id = xorshift64(
        id.wrapping_add(meta.st_size())
            .wrapping_add(meta.st_ino() as u64),
    );

    let id = xorshift64(
        id.wrapping_add(meta.st_dev())
            .wrapping_add(meta.st_mtime_nsec() as u64),
    );

    format!("i{:0>16x}", id)
}

pub fn entry_grid_bg_stylesheet(entries: &Vec<(PathBuf, Metadata)>) -> Markup {
    let mut stylesheet = vec![];

    stylesheet.push(String::from(
        ".entry-img-inner::before{\
            content:'';\
            display:block;\
            width:calc(100% + 4px);\
            height:calc(100% + 4px);\
            transform:translate(-2px,-2px);\
            filter:blur(10px)contrast(1.2);\
            background-size:cover;\
            background-repeat:no-repeat;\
        }",
    ));

    for entry in entries {
        let (path, meta) = entry;
        let file_type = FileType::from(path);

        if matches!(file_type, FileType::Image(_)) {
            let id = file_hash_id(meta);

            let path = path
                .strip_prefix(PWD.read().unwrap().as_os_str())
                .unwrap_or(&path)
                .as_os_str()
                .to_string_lossy()
                .to_string();

            let path = urlencoding::encode(&path);

            stylesheet.push(format!(
                "#{}::before{{\
                    background-image:url('/!_/{}');\
                }}",
                id, path
            ));
        }
    }

    html! {
        style { (stylesheet.join("\n")) }
    }
}

pub fn entry_grid(args: &Args, entries: Vec<(PathBuf, Metadata)>) -> Markup {
    html! {
        (entry_grid_bg_stylesheet(&entries))
        div class="entry-grid" {
            @for (path, meta) in entries {
                (entry(args, path, meta))
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FileType {
    Dir,
    Image(String),
    Video(String),
    Unknown(String),
}

impl From<&PathBuf> for FileType {
    fn from(path: &PathBuf) -> Self {
        if path.is_dir() {
            return FileType::Dir;
        }

        let ext = path
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();

        // actix dumdum
        match ext {
            "ts" | "d.ts" | "mts" | "d.mts" => {
                return FileType::Unknown("text/typescript".to_string())
            }
            _ => {}
        }

        let mime = actix_files::file_extension_to_mime(ext);

        match mime.type_() {
            mime::IMAGE => return FileType::Image(mime.to_string()),
            mime::VIDEO => return FileType::Video(mime.to_string()),
            _ => FileType::Unknown(mime.to_string()),
        }
    }
}

fn xorshift64(mut state: u64) -> u64 {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state
}

pub fn entry(args: &Args, path: PathBuf, meta: Metadata) -> Markup {
    let file_name = path.iter().last().unwrap().to_str().unwrap();
    let file_type = FileType::from(&path);

    let pwd = PWD.read().unwrap();

    let path = path
        .strip_prefix(pwd.as_os_str())
        .unwrap_or(&path)
        .as_os_str()
        .to_string_lossy()
        .to_string();

    let is_img = matches!(file_type, FileType::Image(_));

    let path = urlencoding::encode(&path);

    let id = file_hash_id(&meta);

    html! {
        div
        class=(if is_img { "entry img" } else { "entry" })
        {
            @match file_type {
                FileType::Dir => {
                    a
                    class=(if args.traverse { "dir" } else { "dir disabled" })
                    href=(format!("/{}", path.replace("%2F", "/"))) {
                        (icon("folder", 96))
                        span class="name" { (file_name) }
                    }
                }
                FileType::Image(_) => {
                    div class="entry-img-inner" id=(id) {
                        img src=(format!("/!_/{}", path));
                    }
                }
                FileType::Video(mime) => {
                    video controls height="100%" width="100%" {
                        source src=(format!("!_/{}", path)) type=(mime);
                    }
                }
                FileType::Unknown(mime) => {
                    a
                    class="unknown"
                    href=(format!("/!_/{}", path)) {
                        (icon("description", 96))
                        span class="name" { (file_name) }
                        span class="name" { (mime) }
                    }
                }
            }
        }
    }
}
