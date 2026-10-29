use assert_cmd::Command;
use predicates::prelude::*;

fn logcli() -> Command {
    Command::cargo_bin("logcli").expect("logcli binary should build")
}

#[test]
fn renders_json_entries() {
    logcli()
        .arg("tests/fixtures/simple.jsonl")
        .assert()
        .success()
        .stdout(predicate::str::contains("server started"))
        .stdout(predicate::str::contains("INFO"))
        .stdout(predicate::str::contains("component=api"));
}

#[test]
fn filters_by_level() {
    let output = logcli()
        .args(["-l", "error", "tests/fixtures/simple.jsonl"])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");

    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("connection refused"));
    assert!(!stdout.contains("server started"));
}

#[test]
fn filters_nested_fields() {
    let output = logcli()
        .args(["-f", "request.method=GET", "tests/fixtures/simple.jsonl"])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");

    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.contains("slow request"));
}

#[test]
fn absent_modifier_includes_missing_fields() {
    let output = logcli()
        .args(["-f", "status?!=500", "tests/fixtures/simple.jsonl"])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");

    assert_eq!(stdout.lines().count(), 3);
}

#[test]
fn parses_logfmt() {
    logcli()
        .arg("tests/fixtures/app.log")
        .assert()
        .success()
        .stdout(predicate::str::contains("server started"))
        .stdout(predicate::str::contains("INFO"));
}

#[test]
fn skips_invalid_json() {
    logcli()
        .arg("tests/fixtures/malformed.log")
        .assert()
        .success()
        .stdout(predicate::str::contains("good"))
        .stdout(predicate::str::contains("broken").not());
}

#[test]
fn raw_mode_round_trips() {
    let expected = std::fs::read_to_string("tests/fixtures/simple.jsonl").expect("read fixture");
    let output = logcli()
        .args(["-r", "tests/fixtures/simple.jsonl"])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).expect("utf8"), expected);
}

#[test]
fn sorted_output_is_chronological() {
    let output = logcli()
        .args(["-s", "tests/fixtures/sort.log"])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");

    let first = stdout.find("first").expect("first present");
    let second = stdout.find("second").expect("second present");
    let third = stdout.find("third").expect("third present");

    assert!(first < second && second < third);
}

#[test]
fn no_ansi_when_piped() {
    let output = logcli()
        .arg("tests/fixtures/simple.jsonl")
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn missing_file_exits_2() {
    logcli()
        .arg("tests/fixtures/does-not-exist.log")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot"));
}

#[test]
fn invalid_filter_exits_1() {
    logcli()
        .args(["-f", "no-operator-here", "tests/fixtures/simple.jsonl"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid filter"));
}

#[test]
fn hides_requested_fields() {
    logcli()
        .args(["--hide", "component", "tests/fixtures/simple.jsonl"])
        .assert()
        .success()
        .stdout(predicate::str::contains("component=").not());
}

#[test]
fn supports_stdin() {
    logcli()
        .write_stdin("{\"level\":\"info\",\"message\":\"piped\"}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("piped"));
}

#[test]
fn allow_prefix_parses_prefixed_json() {
    logcli()
        .args(["--allow-prefix", "tests/fixtures/prefixed.log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ready"))
        .stdout(predicate::str::contains("component=api"));
}

#[test]
fn since_filters_entries_without_timestamps() {
    let output = logcli()
        .args([
            "--since",
            "2026-10-28T10:00:02Z",
            "tests/fixtures/simple.jsonl",
        ])
        .output()
        .expect("run logcli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");

    assert!(stdout.contains("cache hit"));
    assert!(!stdout.contains("server started"));
}
