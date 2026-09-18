use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::*;

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn parse(value: Value) -> Schedule {
    let mut spec: ScheduleSpec = serde_json::from_value(value).unwrap();
    super::super::validate_schedule(&mut spec).unwrap();
    Schedule::parse(&spec).unwrap()
}

#[test]
fn cron_matches_all_fields_and_preserves_original_weekday_steps() {
    let cases = [
        ("0 9 * * 0", "2026-09-13T09:00:00Z"),
        ("0 9 * * */2", "2026-09-07T09:00:00Z"),
        ("0 9 * * 1-5/2", "2026-09-07T09:00:00Z"),
        ("0 9 13 * 1", "2027-09-13T09:00:00Z"),
        ("0 0 last feb *", "2027-02-28T00:00:00Z"),
        ("0 0 29 feb *", "2028-02-29T00:00:00Z"),
        ("5-15/5 9,12 * * *", "2026-09-07T09:05:00Z"),
    ];
    for (expression, expected) in cases {
        let schedule = parse(json!({"cron":expression}));
        assert_eq!(
            schedule.next(instant("2026-09-07T00:00:00Z"), true),
            Some(instant(expected)),
            "{expression}"
        );
    }
    let named = parse(json!({"cron":"0 9 * * mon-fri/2"}));
    assert_eq!(
        named.next(instant("2026-09-07T09:00:00Z"), false),
        Some(instant("2026-09-08T09:00:00Z"))
    );
    let impossible = parse(json!({"cron":"0 0 31 feb *"}));
    assert_eq!(impossible.next(instant("2026-01-01T00:00:00Z"), true), None);
}

#[test]
fn timezone_and_dst_follow_original_apscheduler_slots() {
    let cases = [
        (
            "0 9 * * *",
            "Asia/Shanghai",
            "2026-09-07T00:00:00Z",
            "2026-09-07T01:00:00Z",
        ),
        (
            "30 2 * * *",
            "America/New_York",
            "2026-03-08T05:00:00Z",
            "2026-03-08T07:30:00Z",
        ),
        (
            "30 1 * * *",
            "America/New_York",
            "2026-11-01T05:31:00Z",
            "2026-11-01T06:30:00Z",
        ),
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-10-25T00:31:00Z",
            "2026-10-25T01:30:00Z",
        ),
    ];
    for (expression, zone, from, expected) in cases {
        let schedule = parse(json!({"cron":expression,"timezone":zone}));
        assert_eq!(schedule.next(instant(from), true), Some(instant(expected)));
        let next = schedule.next(instant(expected), false).unwrap();
        assert!(next > instant(expected));
    }
}

#[test]
fn one_shot_and_fixed_day_repeats_preserve_limits_and_precision() {
    let once = parse(
        json!({"type":"once", "run_at":"2026-09-07T09:00:00.125", "timezone":"Asia/Shanghai"}),
    );
    assert_eq!(
        once.initial(instant("2026-09-08T00:00:00Z")),
        Some(instant("2026-09-07T01:00:00.125Z"))
    );
    assert_eq!(once.next(instant("2026-09-07T01:00:00.125Z"), false), None);
    let repeated = parse(
        json!({"type":"once", "run_at":"2026-03-07T09:00:00-05:00", "timezone":"America/New_York", "repeat_every_days":1, "repeat_end_type":"count", "repeat_count":3}),
    );
    assert_eq!(
        repeated.initial(instant("2026-03-07T14:00:00.001Z")),
        Some(instant("2026-03-08T14:00:00Z"))
    );
    assert_eq!(
        repeated.next(instant("2026-03-09T14:00:00Z"), true),
        Some(instant("2026-03-09T14:00:00Z"))
    );
    assert_eq!(repeated.next(instant("2026-03-09T14:00:00Z"), false), None);
    let until = parse(
        json!({"type":"once", "run_at":"2026-09-07T00:00:00Z", "repeat_every_days":2, "repeat_end_type":"until", "repeat_until":"2026-09-11T00:00:00Z"}),
    );
    assert_eq!(
        until.next(instant("2026-09-10T00:00:00Z"), true),
        Some(instant("2026-09-11T00:00:00Z"))
    );
    assert_eq!(until.next(instant("2026-09-11T00:00:00Z"), false), None);
}

#[test]
fn invalid_fields_and_time_overflow_fail_closed() {
    for expression in [
        "60 0 * * *",
        "0 24 * * *",
        "0 0 0 * *",
        "0 0 * 13 *",
        "0 0 * * nope",
        "*/0 * * * *",
        "1-0 * * * *",
        "0 0 * * sun-mon",
        "0 0 * jan-nope *",
    ] {
        let spec: ScheduleSpec = serde_json::from_value(json!({"cron":expression})).unwrap();
        assert!(Schedule::parse(&spec).is_err(), "{expression}");
    }
    let spec: ScheduleSpec = serde_json::from_value(json!({"type":"once", "run_at":"9999-12-31T00:00:00Z", "repeat_every_days":4_294_967_295_u32, "repeat_end_type":"count", "repeat_count":4_294_967_295_u32})).unwrap();
    assert!(Schedule::parse(&spec).is_err());
    assert!(datetime("2026-13-01T00:00:00", chrono_tz::UTC).is_err());
}

#[test]
#[ignore = "requires the qwenpaw conda Python/legacy APScheduler; reference only, never a runtime dependency"]
fn native_slots_match_the_original_python_scheduler() {
    use std::io::Write as _;
    let mut cases = Vec::new();
    for expression in [
        "*/7 * * * *",
        "5-15/5 9,12 * * *",
        "0 9 13 * 1",
        "0 9 * * */2",
        "0 9 * * 1-5/2",
        "0 0 last feb *",
        "0 0 29 feb *",
        "0 9 * jan-mar/2 mon-fri",
    ] {
        cases.push(
            json!({"schedule":{"cron":expression},"now":"2026-09-07T00:00:00+00:00","count":4}),
        );
    }
    for (zone, expression, now) in [
        ("Asia/Shanghai", "0 9 * * *", "2026-09-07T00:00:00+00:00"),
        (
            "America/New_York",
            "30 2 * * *",
            "2026-03-08T05:00:00+00:00",
        ),
        (
            "America/New_York",
            "30 1 * * *",
            "2026-11-01T05:00:00+00:00",
        ),
        ("Europe/Berlin", "30 2 * * *", "2026-10-25T00:31:00+00:00"),
        (
            "Australia/Lord_Howe",
            "15 2 * * *",
            "2026-10-03T13:00:00+00:00",
        ),
    ] {
        cases.push(json!({"schedule":{"cron":expression,"timezone":zone},"now":now,"count":4}));
    }
    cases.extend([
        json!({"schedule":{"type":"once","run_at":"2026-09-07T09:00:00.125", "timezone":"Asia/Shanghai"},"now":"2026-09-08T00:00:00+00:00","count":4}),
        json!({"schedule":{"type":"once","run_at":"2026-03-07T09:00:00-05:00", "timezone":"America/New_York","repeat_every_days":1,"repeat_end_type":"count","repeat_count":3},"now":"2026-03-07T14:00:00.001+00:00","count":4}),
        json!({"schedule":{"type":"once","run_at":"2026-09-07T00:00:00+00:00","repeat_every_days":2,"repeat_end_type":"until","repeat_until":"2026-09-11T00:00:00+00:00"},"now":"2026-09-07T00:00:00+00:00","count":4}),
    ]);
    let mut actual = Vec::new();
    for case in &mut cases {
        let mut spec: ScheduleSpec = serde_json::from_value(case["schedule"].clone()).unwrap();
        super::super::validate_schedule(&mut spec).unwrap();
        case["schedule"] = serde_json::to_value(&spec).unwrap();
        let schedule = Schedule::parse(&spec).unwrap();
        let mut next = schedule.initial(instant(case["now"].as_str().unwrap()));
        let mut slots = Vec::new();
        for _ in 0..4 {
            slots.push(next.map(|value| value.to_rfc3339()));
            let Some(value) = next else { break };
            next = schedule.next(value, false);
        }
        actual.push(slots);
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = std::process::Command::new("python")
        .arg(root.join("scripts/cron_reference.py"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: Vec<Vec<Option<String>>> =
        serde_json::from_value(reference["slots"].clone()).unwrap();
    // Compare instants rather than formatting (Python emits six fractional digits).
    let normalize = |slots: Vec<Vec<Option<String>>>| {
        slots
            .into_iter()
            .map(|slots| {
                slots
                    .into_iter()
                    .map(|value| value.map(|value| instant(&value)))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(normalize(actual), normalize(expected));
    println!(
        "APScheduler {}: {} schedule cases matched",
        reference["apscheduler"],
        cases.len()
    );
}
