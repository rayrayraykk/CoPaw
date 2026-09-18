use super::*;
use pretty_assertions::assert_eq;

fn version_examples() -> Vec<String> {
    let mut versions: Vec<String> = [
        "",
        "1",
        "1.0",
        "1.0.0",
        "0001.00.0",
        "1a",
        "1a0",
        "1alpha1",
        "1beta1",
        "1b1",
        "1c1",
        "1pre1",
        "1preview1",
        "1rc1",
        "1rc2",
        "1post",
        "1post0",
        "1post1",
        "1rev1",
        "1r1",
        "1-1",
        "1dev",
        "1dev0",
        "1dev1",
        "1rc1.post2.dev3",
        "1+0",
        "1+00",
        "1+0.0",
        "1+abc10",
        "1+abc2",
        "1+0abc10",
        "1+0abc2",
        "1+123abc",
        "1+z",
        "1+ABC.1",
        "1+abc_1",
        "1+abc-1",
        "1+1abc",
        "1+001abc",
        "V1.0",
        " v1.0 ",
        "\u{a0}1.0\u{a0}",
        "\u{1c}1.0\u{1f}",
        "0!1.0",
        "1!1.0",
        "2!1.0",
        "2.2.0b5",
        "2.2.0",
        "3.0",
        "1.١",
        "１.0",
        "1+١",
        "not-version",
        "1..0",
        "1+",
        "1.*",
        "1.0+alpha+1",
        "1.0+abc..1",
        "1.0..post1",
        "1.0+💡",
        "1.0 a1",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for number in [
        "18446744073709551615",
        "18446744073709551616",
        "018446744073709551616",
        "18446744073709551617",
    ] {
        for version in [
            format!("1.{number}"),
            format!("{number}!1"),
            format!("1a{number}"),
            format!("1.post{number}"),
            format!("1.dev{number}"),
            format!("1+{number}"),
            format!("1+abc.{number}"),
        ] {
            versions.push(version);
        }
    }
    let huge = "9".repeat(300);
    versions.extend([
        format!("0.{huge}"),
        format!("1.{huge}"),
        format!("1+{huge}"),
        format!("1+0{huge}"),
        format!("1post{huge}"),
        format!("1a{huge}"),
        format!("1dev{huge}"),
    ]);
    versions
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original version decisions"]
async fn catalog_versions_full_matrix_matches_original_python() {
    let versions = version_examples();
    let mut entries = Vec::new();
    for version in &versions {
        for entry in [
            json!({"id":"demo","min_version":version}),
            json!({"id":"demo","min_version":"1.0","max_version":version}),
            json!({"id":"demo","qwenpaw_version":{"min":version,"max":"3.0"}}),
        ] {
            entries.push(entry);
        }
    }
    let expected = json!({
        "upgrades":versions.iter().map(|left| versions.iter().map(|right| upgrade(left,right)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "compatible":entries.iter().map(compatible).collect::<Vec<_>>()
    });
    let input = json!({"versions":versions,"entries":entries});
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("python")
            .current_dir(root)
            .args(["-m", "scripts.official_plugin_version_reference"])
            .arg(input.to_string())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    // Identify a divergent pair before the complete structural assertion.
    for (i, left) in versions.iter().enumerate() {
        for (j, right) in versions.iter().enumerate() {
            assert_eq!(
                expected["upgrades"][i][j], actual["upgrades"][i][j],
                "{left:?} -> {right:?}"
            );
        }
    }
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            expected["compatible"][i], actual["compatible"][i],
            "{entry}"
        );
    }
    assert_eq!(expected, actual);
    println!(
        "catalog version reference: {} versions, {} upgrade comparisons, {} compatibility entries",
        versions.len(),
        versions.len() * versions.len(),
        entries.len()
    );
}

#[test]
fn catalog_versions_compare_large_numeric_components_without_string_fallback() {
    let huge = "18446744073709551616";
    for (installed, available) in [
        (format!("1.{huge}"), String::from("1.2")),
        (format!("{huge}!1"), String::from("2!1")),
        (format!("1a{huge}"), String::from("1a2")),
        (format!("1.post{huge}"), String::from("1.post2")),
        (format!("1.dev{huge}"), String::from("1.dev2")),
        (format!("1+{huge}"), String::from("1+2")),
        (format!("1+{huge}"), String::from("1+z")),
    ] {
        assert_eq!(
            upgrade(&installed, &available),
            false,
            "{installed} -> {available}"
        );
        assert_eq!(
            upgrade(&available, &installed),
            true,
            "{available} -> {installed}"
        );
    }
    assert!(!upgrade(
        "1.18446744073709551616",
        "1.018446744073709551616.0"
    ));
    assert_eq!(upgrade("1+0abc10", "1+0abc2"), true);
}

#[test]
fn catalog_versions_preserve_zero_padding_and_python_outer_whitespace() {
    for installed in ["1.0", "01.00.000", "\u{a0}1.0\u{a0}", "\u{1c}1.0\u{1f}"] {
        assert_eq!(upgrade(installed, "1.0"), false, "{installed:?}");
    }
    assert_eq!(upgrade("1.0a0", "1.0a"), false);
    assert_eq!(upgrade("1.0", "1.0+0"), true);
}

#[test]
fn catalog_versions_validate_large_maximum_without_enforcing_its_upper_bound() {
    assert!(compatible(
        &json!({"id":"demo","min_version":"2.2.0","max_version":"18446744073709551616.0"})
    ));
    assert!(!compatible(
        &json!({"id":"demo","min_version":"18446744073709551616.0"})
    ));
    assert!(!compatible(&json!({"id":"demo","min_version":"1.0a1"})));
    assert!(compatible(&json!({"id":"demo","min_version":"1.0.0a1"})));
}
