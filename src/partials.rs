use maud::{html, Markup, DOCTYPE};

pub fn header(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        link rel="stylesheet" href="/_!/reset.css";
        link rel="stylesheet" href="/_!/iv.css";

        head {
            title { (page_title) }
        }
    }
}

pub fn footer() -> Markup {
    html! {
        footer {
            p { "This is a footer." }
        }
    }
}

pub fn page(page_title: &str, pwd: &str, content: Markup) -> Markup {
    html! {
        (header(page_title))
        body {
            div class="container" {
                header class="header" {
                    h1 { (page_title) }
                    p { (format!("path: {}", pwd)) }
                }

                (content)

                (footer())
            }
        }
    }
}
