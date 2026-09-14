//! Standalone fixture executable, compiled by the transaction regression test.
use std::env;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Duration;

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let root = PathBuf::from(env::var_os("LEDGER_TEST_ROOT").unwrap());
    let boundary = env::var("LEDGER_TEST_BOUNDARY").unwrap();
    let first = args.first().and_then(|arg| arg.to_str());
    let hit = match boundary.as_str() {
        "metadata" => first == Some("metadata") && env::current_dir().unwrap() == root,
        "locate" => {
            first == Some("locate-project")
                && args.contains(&root.join("a/Cargo.toml").into_os_string())
        }
        "second-package" => {
            first == Some("check")
                && args
                    .windows(2)
                    .any(|pair| pair[0] == "--package" && pair[1] == "b")
        }
        _ => panic!("unknown boundary"),
    };
    let output = Command::new(env::var_os("LEDGER_TEST_CARGO").unwrap())
        .args(&args)
        .output()
        .unwrap();
    if hit && output.status.success() {
        let mut stream = UnixStream::connect(env::var_os("LEDGER_TEST_SOCKET").unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream.write_all(&[1]).unwrap();
        let mut resume = [0; 1];
        stream.read_exact(&mut resume).unwrap();
        assert_eq!(resume, [1]);
    }
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    process::exit(output.status.code().unwrap_or(1));
}
