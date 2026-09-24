//! Development twin of `cmduse`: same code, different binary name, so a local
//! build never shadows the Homebrew-installed `cmduse`. Consumers (mpc) can
//! target it with `CMDUSE_BIN=cmdusedev`.
fn main() {
    cmd_usage::run();
}
