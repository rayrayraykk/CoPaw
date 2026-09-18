use super::*;

fn stdio_server(root: &Path) -> McpServerDefinition {
    McpServerDefinition {
        name: "stdio-one".to_owned(),
        display_name: "Display".to_owned(),
        command: "tool with spaces".to_owned(),
        args: vec![
            "--name".to_owned(),
            "技能 😀".to_owned(),
            "\t\n\"\\\u{7f}".to_owned(),
        ],
        cwd: Some(root.join("tools")),
        env: IndexMap::from([
            ("SHARED".to_owned(), "fake-secret-one".to_owned()),
            ("EMPTY".to_owned(), String::new()),
        ]),
        tools: Some(
            ["blocked", "read", "ask", "read", "tool.with.dot", "技能"]
                .map(str::to_owned)
                .to_vec(),
        ),
        tool_policies: BTreeMap::from([
            ("blocked".to_owned(), ToolPolicy::Deny),
            ("read".to_owned(), ToolPolicy::Allow),
            ("ask".to_owned(), ToolPolicy::Ask),
            ("tool.with.dot".to_owned(), ToolPolicy::Allow),
        ]),
        credential_revision: "v1".to_owned(),
        runtime_revision: "resolved-v1".to_owned(),
        ..McpServerDefinition::default()
    }
}

pub(crate) fn cases() -> Vec<RuntimeCapabilities> {
    let root = std::env::temp_dir().join("qwenpaw projection 技能");
    let stdio = stdio_server(&root);
    let http = McpServerDefinition {
        name: "服务.a/😀".to_owned(),
        display_name: "HTTP".to_owned(),
        transport: McpTransport::StreamableHttp,
        url: "https://example.invalid/mcp".to_owned(),
        headers: BTreeMap::from([
            ("Authorization".to_owned(), "fake-bearer".to_owned()),
            ("X-名".to_owned(), "header-fixture".to_owned()),
            ("x:y".to_owned(), "one".to_owned()),
            ("x/y".to_owned(), "two".to_owned()),
        ]),
        default_policy: ToolPolicy::Allow,
        ..McpServerDefinition::default()
    };
    let mut values = vec![
        RuntimeCapabilities::default(),
        RuntimeCapabilities {
            skills: vec![
                SkillDefinition {
                    name: "review".to_owned(),
                    description: "not identity".to_owned(),
                    directory: root.join("review 😀"),
                    revision: "skill-v1".to_owned(),
                },
                SkillDefinition {
                    name: "review".to_owned(),
                    directory: root.join("other"),
                    ..SkillDefinition::default()
                },
            ],
            mcp_servers: vec![stdio.clone(), http.clone()],
        },
    ];
    let mut sse = http;
    sse.transport = McpTransport::Sse;
    sse.default_policy = ToolPolicy::Deny;
    sse.tool_policies
        .insert("override".to_owned(), ToolPolicy::Allow);
    values.push(RuntimeCapabilities {
        mcp_servers: vec![sse],
        ..RuntimeCapabilities::default()
    });
    for name in [
        "",
        "---",
        "中文",
        "a.b",
        "a/b",
        "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz.",
    ] {
        values.push(RuntimeCapabilities {
            mcp_servers: vec![McpServerDefinition {
                name: name.to_owned(),
                display_name: name.to_owned(),
                tools: Some(vec![]),
                ..McpServerDefinition::default()
            }],
            ..RuntimeCapabilities::default()
        });
    }
    let mut second = stdio.clone();
    second.name = "second".to_owned();
    values.push(RuntimeCapabilities {
        mcp_servers: vec![stdio.clone(), second.clone()],
        ..RuntimeCapabilities::default()
    });
    second
        .env
        .insert("EMPTY".to_owned(), "different-fake-value".to_owned());
    values.push(RuntimeCapabilities {
        mcp_servers: vec![stdio, second],
        ..RuntimeCapabilities::default()
    });
    let mut first = McpServerDefinition {
        name: "first".to_owned(),
        display_name: "first".to_owned(),
        env: IndexMap::from([
            ("Z".to_owned(), "first-z".to_owned()),
            ("A".to_owned(), "first-a".to_owned()),
        ]),
        ..McpServerDefinition::default()
    };
    let mut second = first.clone();
    second.name = "second".to_owned();
    second.env.insert("Z".to_owned(), "second-z".to_owned());
    second.env.insert("A".to_owned(), "second-a".to_owned());
    first.default_policy = ToolPolicy::Allow;
    values.insert(
        values.len() - 1,
        RuntimeCapabilities {
            mcp_servers: vec![first, second],
            ..RuntimeCapabilities::default()
        },
    );
    values
}

pub(crate) fn fixture(capabilities: &RuntimeCapabilities) -> Value {
    json!({"skills":capabilities.skills.iter().map(|skill| json!({
        "name":skill.name,"description":skill.description,"directory":skill.directory,"revision":skill.revision
    })).collect::<Vec<_>>(),"mcp_servers":capabilities.mcp_servers.iter().map(|server| json!({
        "name":server.name,"display_name":server.display_name,"transport":server.transport,
        "command":server.command,"args":server.args,"cwd":server.cwd,"url":server.url,
        "env":server.env,"env_entries":server.env.iter().collect::<Vec<_>>(),"headers":server.headers,"tools":server.tools,"tool_policies":server.tool_policies,
        "default_policy":server.default_policy,"credential_revision":server.credential_revision,"runtime_revision":server.runtime_revision
    })).collect::<Vec<_>>()})
}

#[test]
fn ascii_json_matches_python_spacing_sorting_and_surrogate_pairs() {
    let input = json!({"z":["😀\u{7f}","\n\t\"\\"],"é":{"a":true,"b":null}});
    assert_eq!(
        json_ascii(&input, false),
        r#"{"z":["\ud83d\ude00\u007f","\n\t\"\\"],"\u00e9":{"a":true,"b":null}}"#
    );
    assert_eq!(
        json_ascii(&json!(["a", "中文", ""]), true),
        r#"["a", "\u4e2d\u6587", ""]"#
    );
}

#[test]
fn fingerprint_ignores_display_fields_and_values_until_revisions_refresh() {
    let original = cases().remove(1);
    let initial = original.fingerprint().unwrap();
    let mut changed = original.clone();
    changed.skills[0].description = "changed display".to_owned();
    changed.mcp_servers[0].display_name = "changed name".to_owned();
    changed.mcp_servers[0]
        .env
        .insert("SHARED".to_owned(), "rotated-fake-secret".to_owned());
    assert_eq!(changed.fingerprint().unwrap(), initial);
    changed.mcp_servers[0].refresh_runtime_revision();
    assert_ne!(changed.fingerprint().unwrap(), initial);
    assert!(
        !changed.mcp_servers[0]
            .runtime_revision
            .contains("rotated-fake-secret")
    );
    let mut refreshed = original;
    refreshed.mcp_servers[0].refresh_runtime_revision();
    assert_ne!(
        changed.mcp_servers[0].runtime_revision,
        refreshed.mcp_servers[0].runtime_revision
    );
}

#[test]
fn identity_preserves_list_order_tools_revision_and_env_key_changes() {
    let original = cases().remove(1);
    let fingerprint = original.fingerprint().unwrap();
    let mut changed = original.clone();
    changed.mcp_servers.reverse();
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    changed = original.clone();
    changed.skills.reverse();
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    changed = original.clone();
    changed.skills[0].revision.push('2');
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    changed = original.clone();
    changed.mcp_servers[0].credential_revision.push('2');
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    changed = original.clone();
    changed.mcp_servers[0]
        .env
        .insert("NEW".to_owned(), String::new());
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
    changed = original;
    changed.mcp_servers[0].tools = None;
    assert_ne!(changed.fingerprint().unwrap(), fingerprint);
}

#[cfg(unix)]
#[test]
fn invalid_unicode_paths_are_not_lossily_hashed() {
    use std::os::unix::ffi::OsStrExt;
    let invalid = PathBuf::from(std::ffi::OsStr::from_bytes(b"path-\xff"));
    let value = RuntimeCapabilities {
        skills: vec![SkillDefinition {
            directory: invalid,
            ..SkillDefinition::default()
        }],
        ..RuntimeCapabilities::default()
    };
    assert_eq!(value.fingerprint(), Err(CapabilityError::InvalidPath));
}

#[test]
fn environment_insertion_order_does_not_change_sorted_identity_or_value_revision() {
    let mut first = cases().remove(1);
    let mut second = first.clone();
    second.mcp_servers[0].env.reverse();
    assert_eq!(first.fingerprint().unwrap(), second.fingerprint().unwrap());
    first.mcp_servers[0].refresh_runtime_revision();
    second.mcp_servers[0].refresh_runtime_revision();
    assert_eq!(
        first.mcp_servers[0].runtime_revision,
        second.mcp_servers[0].runtime_revision
    );
    assert_eq!(first.fingerprint().unwrap(), second.fingerprint().unwrap());
}
