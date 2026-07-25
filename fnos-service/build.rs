use time::OffsetDateTime;
use vergen_gix::{Emitter, GixBuilder};

fn main() {
    let gix = GixBuilder::default().sha(true).build().unwrap();
    Emitter::new().add_instructions(&gix).unwrap().emit_and_set().unwrap();
    let sha = std::env::var("VERGEN_GIT_SHA").unwrap_or_default();
    let timestamp = OffsetDateTime::now_utc()
        .format(time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z"))
        .unwrap_or_default();
    let version = format!("{}-{} ({})", env!("CARGO_PKG_VERSION"), sha, timestamp);
    println!("cargo:rustc-env=VERSION={}", version);
}
