use assert_cmd::Command;
use predicates::prelude::*;

fn logcli() -> Command {
    Command::cargo_bin("logcli").expect("logcli binary should build")
}

const FIXTURE: &str = "tests/fixtures/query.jsonl";

fn run_query(query: &str) -> assert_cmd::assert::Assert {
    logcli().arg("-q").arg(query).arg(FIXTURE).assert()
}

fn run_args(args: &[&str]) -> assert_cmd::assert::Assert {
    logcli().args(args).assert()
}

fn stdout_of(args: &[&str]) -> String {
    let output = logcli().args(args).output().expect("run logcli");
    assert!(
        output.status.success(),
        "logcli {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8")
}

#[test]
fn query_level_equality() {
    run_query("level = error")
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query("level != error")
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_level_ordering() {
    run_query("level >= warn")
        .success()
        .stdout(predicate::str::contains("connection refused"))
        .stdout(predicate::str::contains("slow request"));
}

#[test]
fn query_level_in_set() {
    run_query("level in (warn, error, fatal)")
        .success()
        .stdout(predicate::str::contains("slow request"))
        .stdout(predicate::str::contains("connection refused"));
}

#[test]
fn query_message_contains() {
    run_query(r#"message ~= "connection refused""#)
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query(r#"message ~= "cache""#)
        .success()
        .stdout(predicate::str::contains("cache hit"));
}

#[test]
fn query_message_regex() {
    run_query(r#"message ~~= "timeout|deadline""#)
        .success()
        .stdout(predicate::str::contains("request timeout"));

    run_query(r#"msg !~~= "(?i)deprecat""#)
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_nested_path() {
    run_query("request.method = GET")
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query(r#""request.method" = GET"#)
        .success()
        .stdout(predicate::str::contains("deprecated API"));
}

#[test]
fn query_nested_contains() {
    run_query("request.headers.content-type ~= json")
        .success()
        .stdout(predicate::str::contains("connection refused"));
}

#[test]
fn query_numeric_comparisons() {
    run_query("status >= 500")
        .success()
        .stdout(predicate::str::contains("connection refused"))
        .stdout(predicate::str::contains("request timeout"));

    run_query("duration > 0.5")
        .success()
        .stdout(predicate::str::contains("slow request"))
        .stdout(predicate::str::contains("request timeout"));

    run_query("bytes < 1024")
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_boolean_comparison() {
    run_query("cached = true")
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query("retry != false")
        .success()
        .stdout(predicate::str::contains("deprecated API"));
}

#[test]
fn query_absent_modifier() {
    run_query("request.id? = null")
        .success()
        .stdout(predicate::str::contains("cache hit"));

    run_query("internal? != true")
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_exists() {
    run_query("exists(request.id) and not exists(request.internal)")
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_in_inline_set() {
    run_query("status in (200, 201, 204)")
        .success()
        .stdout(predicate::str::contains("server started"));

    run_query("level in (warn, error, fatal)")
        .success()
        .stdout(predicate::str::contains("slow request"));
}

#[test]
fn query_not_in_file_set() {
    run_query("user not in @tests/fixtures/query-blocked.txt")
        .success()
        .stdout(predicate::str::contains("server started"))
        .stdout(predicate::str::contains("connection refused"));

    // A missing set file is an I/O error.
    logcli()
        .args(["-q", "user not in @/nonexistent/set.txt", FIXTURE])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot open"));
}

#[test]
fn query_in_stdin_set_rejected_for_stdin_logs() {
    logcli()
        .arg("-q")
        .arg("ip in @-")
        .write_stdin("")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot read value set from stdin"));
}

#[test]
fn query_in_stdin_set_reads_values_from_stdin() {
    logcli()
        .arg("-q")
        .arg("ip in @-")
        .arg(FIXTURE)
        .write_stdin("10.0.0.1\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("connection refused"))
        .stdout(predicate::str::contains("server started").not());
}

#[test]
fn query_like_wildcard() {
    run_query("path like /api/*/users")
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query("path not like /health*")
        .success()
        .stdout(predicate::str::contains("connection refused"));
}

#[test]
fn query_regex_and_regex_negation() {
    run_query(r#"msg ~~= "^panic""#)
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query("msg !~~= timeout")
        .success()
        .stdout(predicate::str::contains("server started"));
}

#[test]
fn query_arrays_existential_and_indexed() {
    run_query("span.[].name = sp0001")
        .success()
        .stdout(predicate::str::contains("connection refused"))
        .stdout(predicate::str::contains("request timeout"));

    run_query("span.[1].name = sp0001")
        .success()
        .stdout(predicate::str::contains("connection refused"));

    run_query("tags.[0] = production")
        .success()
        .stdout(predicate::str::contains("connection refused"));
}

#[test]
fn query_boolean_composition() {
    run_query("(level > info or status >= 400) and not (path ~= health)")
        .success()
        .stdout(predicate::str::contains("connection refused"))
        .stdout(predicate::str::contains("request timeout"));
}

#[test]
fn query_or_composition() {
    run_query("level in (error, fatal) or duration > 2.0")
        .success()
        .stdout(predicate::str::contains("connection refused"));
}

#[test]
fn multiple_queries_are_anded() {
    run_args(&["-q", "level = error", "-q", "status >= 500", FIXTURE]).success();
}

#[test]
fn filter_and_query_are_anded() {
    stdout_of(&["-f", "status=500", "-q", r#"message ~= "refused""#, FIXTURE])
        .contains("connection refused");
}

#[test]
fn query_aliases() {
    run_query("status eq 200")
        .success()
        .stdout(predicate::str::contains("slow request"));

    run_query("status ne 200").success();
}

#[test]
fn query_invalid_exits_1_with_caret() {
    logcli()
        .args(["-q", "status =", FIXTURE])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid query at column 9"))
        .stderr(predicate::str::contains("expected a value after"));
}

#[test]
fn query_bare_field_path_is_an_error() {
    logcli()
        .arg("-q")
        .arg("error")
        .arg(FIXTURE)
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "expected a comparison, found field path 'error'",
        ));
}

#[test]
fn query_unterminated_string_is_an_error() {
    logcli()
        .arg("-q")
        .arg("\"oops")
        .arg(FIXTURE)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unterminated string literal"));
}

#[test]
fn query_invalid_regex_is_an_error() {
    let pattern = "msg ~~= \"[\"";
    logcli()
        .args(["-q", pattern, FIXTURE])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid regex"));
}
