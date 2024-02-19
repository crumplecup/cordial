// use cordial::prelude::*;
use reqwest::Client;

pub const LOCAL: &str = "http://localhost:8000";

#[derive(Clone)]
pub struct Voice(reqwest::Client);

impl Voice {
    pub fn new() -> Self {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .cookie_store(true)
            .build()
            .unwrap();
        Voice(client)
    }
}
