use crate::error::*;
use log::{trace, warn};
use std::process::Command;
use std::str;

pub fn run_sh(command: &str) -> Result<Vec<String>> {
    trace!("Run shell command: {}", &command);

    Command::new("sh")
        .args(&["-c", &command])
        .output()
        .map(|o| {
            str::from_utf8(&o.stdout)
                .unwrap()
                .lines()
                .map(|l| {
                    trace!("{}", l);
                    l.to_string()
                })
                .collect()
        })
        .map_err(|e| {
            warn!("{:?}", e);
            Error::new("Can't run the sell command", e)
        })
}
