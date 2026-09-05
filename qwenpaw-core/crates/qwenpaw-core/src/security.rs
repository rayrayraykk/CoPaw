use std::collections::BTreeMap;
use std::collections::HashSet;
use std::net::IpAddr;
use std::path::Path;

use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

const BUILTIN_RULES_YAML: &str =
    include_str!("../resources/security/dangerous_shell_commands.yaml");
const CATASTROPHIC_RULE_ID: &str = "SAFETY_CHECKS_DESTRUCTIVE_COMMAND";
const MAX_SECURITY_RULES: usize = 512;
const MAX_RULE_PATTERNS: usize = 64;
const MAX_SECURITY_STRING_BYTES: usize = 4096;
const MAX_BLOCKED_HISTORY: usize = 500;
const SECURITY_DATA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct StoredSecurityData {
    version: u32,
    settings: SecuritySettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolGuardRule {
    pub id: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub params: Vec<String>,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolGuardConfig {
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub guarded_tools: Option<Vec<String>>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default = "default_auto_denied_rules")]
    pub auto_denied_rules: Vec<String>,
    #[serde(default)]
    pub custom_rules: Vec<ToolGuardRule>,
    #[serde(default)]
    pub disabled_rules: Vec<String>,
    #[serde(default = "default_shell_evasion_checks")]
    pub shell_evasion_checks: BTreeMap<String, bool>,
}

impl Default for ToolGuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            guarded_tools: None,
            denied_tools: Vec::new(),
            auto_denied_rules: default_auto_denied_rules(),
            custom_rules: Vec::new(),
            disabled_rules: Vec::new(),
            shell_evasion_checks: default_shell_evasion_checks(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileGuardConfig {
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "enabled")]
    pub allow_preview_outside_workspace: bool,
}

impl Default for FileGuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paths: default_sensitive_paths(),
            allow_preview_outside_workspace: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillScannerMode {
    Block,
    Warn,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillScannerWhitelistEntry {
    pub skill_name: String,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillScannerConfig {
    pub mode: SkillScannerMode,
    pub timeout: u16,
    #[serde(default)]
    pub whitelist: Vec<SkillScannerWhitelistEntry>,
}

impl Default for SkillScannerConfig {
    fn default() -> Self {
        Self {
            mode: SkillScannerMode::Warn,
            timeout: 30,
            whitelist: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedSkillFinding {
    pub severity: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub line_number: Option<u32>,
    pub rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedSkillRecord {
    pub skill_name: String,
    pub blocked_at: String,
    pub max_severity: String,
    #[serde(default)]
    pub findings: Vec<BlockedSkillFinding>,
    #[serde(default)]
    pub content_hash: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecuritySettings {
    #[serde(default)]
    pub tool_guard: ToolGuardConfig,
    #[serde(default)]
    pub file_guard: FileGuardConfig,
    #[serde(default)]
    pub skill_scanner: SkillScannerConfig,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default = "default_allow_no_auth_hosts")]
    pub allow_no_auth_hosts: Vec<String>,
    #[serde(default)]
    pub blocked_skill_history: Vec<BlockedSkillRecord>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            tool_guard: ToolGuardConfig::default(),
            file_guard: FileGuardConfig::default(),
            skill_scanner: SkillScannerConfig::default(),
            sandbox_enabled: false,
            allow_no_auth_hosts: default_allow_no_auth_hosts(),
            blocked_skill_history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityApprovalMode {
    Strict,
    Smart,
    Auto,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolGuardEffect {
    Allow,
    Ask(String),
    Deny(String),
}

#[derive(Debug, Clone)]
pub(crate) struct SecurityPolicy {
    settings: SecuritySettings,
    rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    rule: ToolGuardRule,
    patterns: Vec<Regex>,
    exclusions: Vec<Regex>,
}

impl SecurityPolicy {
    pub(crate) fn new(settings: SecuritySettings) -> Result<Self, String> {
        validate_settings(&settings)?;
        let disabled = settings
            .tool_guard
            .disabled_rules
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut rules = builtin_tool_guard_rules()?;
        rules.extend(settings.tool_guard.custom_rules.clone());
        let rules = rules
            .into_iter()
            .filter(|rule| !disabled.contains(rule.id.as_str()))
            .map(CompiledRule::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { settings, rules })
    }

    pub(crate) fn settings(&self) -> &SecuritySettings {
        &self.settings
    }

    pub(crate) fn sandbox_active(&self) -> bool {
        self.settings.sandbox_enabled && qwenpaw_tools::shell_sandbox_available()
    }

    pub(crate) fn evaluate(
        &self,
        tool_name: &str,
        arguments: &str,
        mode: SecurityApprovalMode,
    ) -> ToolGuardEffect {
        let guard = &self.settings.tool_guard;
        if mode == SecurityApprovalMode::Off || !guard.enabled {
            return ToolGuardEffect::Allow;
        }
        let aliases = tool_aliases(tool_name);
        if guard
            .denied_tools
            .iter()
            .any(|denied| aliases.contains(&denied.as_str()))
        {
            return ToolGuardEffect::Deny(format!(
                "Tool '{tool_name}' is denied by the Tool Guard policy."
            ));
        }
        if mode == SecurityApprovalMode::Strict {
            return ToolGuardEffect::Ask(format!(
                "Strict approval is enabled for tool '{tool_name}'."
            ));
        }
        if !is_guarded(guard, &aliases) {
            return ToolGuardEffect::Allow;
        }
        let searchable = searchable_value(tool_name, arguments);
        if tool_name == "shell"
            && catastrophic_shell_command(&searchable)
            && guard
                .auto_denied_rules
                .iter()
                .any(|id| id == CATASTROPHIC_RULE_ID)
        {
            return ToolGuardEffect::Deny(String::from(
                "Tool call matched SAFETY_CHECKS_DESTRUCTIVE_COMMAND and was denied.",
            ));
        }
        for rule in &self.rules {
            if !rule.matches(&aliases, arguments) {
                continue;
            }
            let message = format!(
                "Tool call matched {}: {}",
                rule.rule.id, rule.rule.description
            );
            if guard.auto_denied_rules.iter().any(|id| id == &rule.rule.id) {
                return ToolGuardEffect::Deny(message);
            }
            if mode == SecurityApprovalMode::Smart
                && matches!(rule.rule.severity.as_str(), "LOW" | "INFO")
            {
                continue;
            }
            return ToolGuardEffect::Ask(message);
        }
        if let Some(check) = enabled_evasion_finding(&searchable, &guard.shell_evasion_checks) {
            return ToolGuardEffect::Ask(format!("Shell evasion check matched: {check}"));
        }
        if let Some(path) = sensitive_file_path(tool_name, arguments, &self.settings.file_guard) {
            return ToolGuardEffect::Ask(format!("File Guard matched sensitive path '{path}'."));
        }
        ToolGuardEffect::Allow
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self::new(SecuritySettings::default()).expect("default Security settings must compile")
    }
}

impl CompiledRule {
    fn new(rule: ToolGuardRule) -> Result<Self, String> {
        let patterns = compile_patterns(&rule.id, &rule.patterns)?;
        let exclusions = compile_patterns(&rule.id, &rule.exclude_patterns)?;
        Ok(Self {
            rule,
            patterns,
            exclusions,
        })
    }

    fn matches(&self, aliases: &[&str], arguments: &str) -> bool {
        if !self.rule.tools.is_empty()
            && !self
                .rule
                .tools
                .iter()
                .any(|tool| aliases.contains(&tool.as_str()))
        {
            return false;
        }
        let Ok(Value::Object(params)) = serde_json::from_str(arguments) else {
            return self.rule.params.is_empty() && self.matches_value(arguments);
        };
        params.iter().any(|(name, value)| {
            (self.rule.params.is_empty() || self.rule.params.contains(name))
                && value.as_str().map_or_else(
                    || !value.is_null() && self.matches_value(&value.to_string()),
                    |value| self.matches_value(value),
                )
        })
    }

    fn matches_value(&self, value: &str) -> bool {
        !self
            .exclusions
            .iter()
            .any(|pattern| pattern.is_match(value))
            && self.patterns.iter().any(|pattern| pattern.is_match(value))
    }
}

/// Loads the built-in Tool Guard rule set embedded in the Rust Core binary.
///
/// # Errors
///
/// Returns an error when an embedded rule cannot be decoded.
pub fn builtin_tool_guard_rules() -> Result<Vec<ToolGuardRule>, String> {
    let normalized =
        BUILTIN_RULES_YAML.replace(r"\\$IFS(?![A-Za-z0-9_])", r"\\$IFS(?:[^A-Za-z0-9_]|$)");
    yaml_serde::from_str(&normalized).map_err(|error| format!("invalid built-in rules: {error}"))
}

pub fn validate_settings(settings: &SecuritySettings) -> Result<(), String> {
    let guard = &settings.tool_guard;
    if guard.custom_rules.len() > MAX_SECURITY_RULES {
        return Err(format!(
            "custom_rules cannot exceed {MAX_SECURITY_RULES} entries"
        ));
    }
    for rule in &guard.custom_rules {
        validate_rule(rule)?;
        CompiledRule::new(rule.clone())?;
    }
    validate_strings(&guard.denied_tools, "denied_tools")?;
    validate_strings(&guard.disabled_rules, "disabled_rules")?;
    validate_strings(&guard.auto_denied_rules, "auto_denied_rules")?;
    if let Some(tools) = &guard.guarded_tools {
        validate_strings(tools, "guarded_tools")?;
    }
    validate_strings(&settings.file_guard.paths, "file_guard.paths")?;
    if !(5..=300).contains(&settings.skill_scanner.timeout) {
        return Err(String::from(
            "skill_scanner.timeout must be between 5 and 300",
        ));
    }
    if settings.blocked_skill_history.len() > MAX_BLOCKED_HISTORY {
        return Err(format!(
            "blocked skill history cannot exceed {MAX_BLOCKED_HISTORY} entries"
        ));
    }
    normalize_ip_hosts(&settings.allow_no_auth_hosts)?;
    Ok(())
}

/// Validates, canonicalizes, and de-duplicates literal IP addresses.
///
/// # Errors
///
/// Returns an error listing every non-empty value that is not a literal IP.
pub fn normalize_ip_hosts(hosts: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    let mut invalid = Vec::new();
    for host in hosts {
        let host = host.trim();
        if host.is_empty() {
            continue;
        }
        match host.parse::<IpAddr>() {
            Ok(address) => {
                let address = address.to_string();
                if seen.insert(address.clone()) {
                    normalized.push(address);
                }
            }
            Err(_) => invalid.push(host.to_owned()),
        }
    }
    if invalid.is_empty() {
        Ok(normalized)
    } else {
        Err(format!(
            "Invalid IP address(es): {}. Only literal IPv4/IPv6 addresses are allowed.",
            invalid.join(", ")
        ))
    }
}

pub(crate) fn decode_security_settings(value: &str) -> Result<SecuritySettings, String> {
    let stored = serde_json::from_str::<StoredSecurityData>(value)
        .map_err(|error| format!("stored security settings are invalid: {error}"))?;
    if stored.version != SECURITY_DATA_VERSION {
        return Err(format!(
            "stored security settings version {} is unsupported",
            stored.version
        ));
    }
    validate_settings(&stored.settings)?;
    Ok(stored.settings)
}

pub(crate) fn encode_security_settings(settings: &SecuritySettings) -> Result<String, String> {
    serde_json::to_string(&StoredSecurityData {
        version: SECURITY_DATA_VERSION,
        settings: settings.clone(),
    })
    .map_err(|error| format!("security settings could not be serialized: {error}"))
}

pub(crate) fn trim_blocked_history(settings: &mut SecuritySettings) {
    if settings.blocked_skill_history.len() > MAX_BLOCKED_HISTORY {
        let excess = settings.blocked_skill_history.len() - MAX_BLOCKED_HISTORY;
        settings.blocked_skill_history.drain(..excess);
    }
}

fn validate_rule(rule: &ToolGuardRule) -> Result<(), String> {
    if rule.id.trim().is_empty() {
        return Err(String::from("custom rule id cannot be empty"));
    }
    if rule.patterns.is_empty() || rule.patterns.len() > MAX_RULE_PATTERNS {
        return Err(format!(
            "custom rule '{}' must contain 1 to {MAX_RULE_PATTERNS} patterns",
            rule.id
        ));
    }
    validate_strings(&rule.tools, "rule.tools")?;
    validate_strings(&rule.params, "rule.params")?;
    validate_strings(&rule.patterns, "rule.patterns")?;
    validate_strings(&rule.exclude_patterns, "rule.exclude_patterns")?;
    Ok(())
}

fn validate_strings(values: &[String], field: &str) -> Result<(), String> {
    if let Some(value) = values
        .iter()
        .find(|value| value.is_empty() || value.len() > MAX_SECURITY_STRING_BYTES)
    {
        return Err(format!("{field} contains an invalid value: {value}"));
    }
    Ok(())
}

fn compile_patterns(rule_id: &str, patterns: &[String]) -> Result<Vec<Regex>, String> {
    patterns
        .iter()
        .map(|pattern| {
            Regex::new(&format!("(?i){pattern}"))
                .map_err(|error| format!("invalid regex in rule '{rule_id}': {error}"))
        })
        .collect()
}

fn tool_aliases(tool_name: &str) -> Vec<&str> {
    match tool_name {
        "shell" => vec!["shell", "execute_shell_command"],
        "replace_text" => vec!["replace_text", "edit_file"],
        "write_file" => vec!["write_file", "write_text_file"],
        "read_file" => vec!["read_file", "view_text_file"],
        name => vec![name],
    }
}

fn is_guarded(config: &ToolGuardConfig, aliases: &[&str]) -> bool {
    match &config.guarded_tools {
        None => true,
        Some(tools) => tools.iter().any(|tool| aliases.contains(&tool.as_str())),
    }
}

fn searchable_value(tool_name: &str, arguments: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(arguments) else {
        return arguments.to_owned();
    };
    if tool_name == "shell" {
        return value
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or(arguments)
            .to_owned();
    }
    arguments.to_owned()
}

fn catastrophic_shell_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    Regex::new(r"\b(mkfs(?:\.[a-z0-9_]+)?|mke2fs)\b")
        .expect("static regex")
        .is_match(&lower)
        || Regex::new(r"\bdd\s+.*\bof=/dev/")
            .expect("static regex")
            .is_match(&lower)
        || Regex::new(r"\brm\s+(?:-[a-z]*r[a-z]*f|-rf|-fr)\s+/(?:\s|$)")
            .expect("static regex")
            .is_match(&lower)
}

fn enabled_evasion_finding(command: &str, checks: &BTreeMap<String, bool>) -> Option<&'static str> {
    let enabled = |name: &str| checks.get(name).copied().unwrap_or(false);
    if enabled("command_substitution") && (command.contains("$(") || command.contains('`')) {
        return Some("command_substitution");
    }
    if enabled("newlines") && (command.contains('\n') || command.contains('\r')) {
        return Some("newlines");
    }
    if enabled("backslash_escaped_whitespace")
        && Regex::new(r"\\[ \t]")
            .expect("static regex")
            .is_match(command)
    {
        return Some("backslash_escaped_whitespace");
    }
    if enabled("backslash_escaped_operators")
        && Regex::new(r"\\[|;&<>]")
            .expect("static regex")
            .is_match(command)
    {
        return Some("backslash_escaped_operators");
    }
    if enabled("obfuscated_flags")
        && Regex::new(r#"(?:'[^']*'|"[^"]*")"#)
            .expect("static regex")
            .is_match(command)
    {
        return Some("obfuscated_flags");
    }
    if enabled("comment_quote_desync")
        && command.contains('#')
        && command.matches('\'').count() % 2 == 1
    {
        return Some("comment_quote_desync");
    }
    if enabled("quoted_newline") && command.contains("\\\n") {
        return Some("quoted_newline");
    }
    None
}

fn sensitive_file_path(
    tool_name: &str,
    arguments: &str,
    config: &FileGuardConfig,
) -> Option<String> {
    if !config.enabled || !matches!(tool_name, "read_file" | "write_file" | "replace_text") {
        return None;
    }
    let parsed = serde_json::from_str::<Value>(arguments).ok()?;
    let path = parsed.get("path")?.as_str()?;
    let candidate = Path::new(path);
    config.paths.iter().find_map(|sensitive| {
        let sensitive_path = Path::new(sensitive);
        (candidate == sensitive_path || candidate.starts_with(sensitive_path))
            .then(|| path.to_owned())
    })
}

fn default_sensitive_paths() -> Vec<String> {
    dirs::home_dir().map_or_else(Vec::new, |home| {
        vec![
            home.join(".qwenpaw.secret").to_string_lossy().into_owned(),
            home.join(".copaw.secret").to_string_lossy().into_owned(),
        ]
    })
}

fn default_shell_evasion_checks() -> BTreeMap<String, bool> {
    [
        "command_substitution",
        "obfuscated_flags",
        "backslash_escaped_whitespace",
        "backslash_escaped_operators",
        "newlines",
        "comment_quote_desync",
        "quoted_newline",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), false))
    .collect()
}

fn default_auto_denied_rules() -> Vec<String> {
    vec![String::from(CATASTROPHIC_RULE_ID)]
}

fn default_allow_no_auth_hosts() -> Vec<String> {
    vec![String::from("127.0.0.1"), String::from("::1")]
}

fn enabled() -> bool {
    true
}

fn default_category() -> String {
    String::from("command_injection")
}

fn default_severity() -> String {
    String::from("HIGH")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_load_with_original_contract() {
        let rules = builtin_tool_guard_rules().expect("rules");
        assert_eq!(rules.len(), 21);
        assert_eq!(rules[0].id, "TOOL_CMD_DANGEROUS_RM");
        assert_eq!(rules[20].id, "TOOL_CMD_ZSH_DANGEROUS");
    }

    #[test]
    fn default_policy_allows_safe_and_asks_for_guarded_command() {
        let policy = SecurityPolicy::default();
        assert_eq!(
            policy.evaluate(
                "shell",
                r#"{"command":"cargo test"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Allow
        );
        assert!(matches!(
            policy.evaluate(
                "shell",
                r#"{"command":"rm old.txt"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Ask(_)
        ));
        assert!(matches!(
            policy.evaluate(
                "shell",
                r#"{"command":"rm old.txt"}"#,
                SecurityApprovalMode::Smart,
            ),
            ToolGuardEffect::Ask(_)
        ));
        assert!(matches!(
            policy.evaluate(
                "read_file",
                r#"{"path":"README.md"}"#,
                SecurityApprovalMode::Strict,
            ),
            ToolGuardEffect::Ask(_)
        ));
        assert_eq!(
            policy.evaluate(
                "shell",
                r#"{"command":"mkfs.ext4 /dev/sda"}"#,
                SecurityApprovalMode::Off,
            ),
            ToolGuardEffect::Allow
        );
        assert!(matches!(
            policy.evaluate(
                "shell",
                r#"{"command":"mkfs.ext4 /dev/sda"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Deny(_)
        ));
    }

    #[test]
    fn denied_alias_custom_rule_and_file_guard_are_enforced() {
        let mut settings = SecuritySettings::default();
        settings.tool_guard.denied_tools = vec![String::from("edit_file")];
        settings.tool_guard.custom_rules.push(ToolGuardRule {
            id: String::from("CUSTOM_TOKEN"),
            tools: vec![String::from("execute_shell_command")],
            params: vec![String::from("command")],
            category: String::from("code_execution"),
            severity: String::from("HIGH"),
            patterns: vec![String::from("danger-token")],
            exclude_patterns: Vec::new(),
            description: String::from("custom finding"),
            remediation: String::new(),
        });
        settings.file_guard.paths = vec![String::from("/secret")];
        let policy = SecurityPolicy::new(settings).expect("policy");
        assert!(matches!(
            policy.evaluate("replace_text", "{}", SecurityApprovalMode::Auto),
            ToolGuardEffect::Deny(_)
        ));
        assert!(matches!(
            policy.evaluate(
                "shell",
                r#"{"command":"echo danger-token"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Ask(_)
        ));
        assert_eq!(
            policy.evaluate(
                "shell",
                r#"{"command":"echo safe","description":"danger-token"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Allow
        );
        assert!(matches!(
            policy.evaluate(
                "read_file",
                r#"{"path":"/secret/key"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Ask(_)
        ));

        let mut settings = SecuritySettings::default();
        settings.tool_guard.custom_rules.push(ToolGuardRule {
            id: String::from("CUSTOM_ALL_TOOLS"),
            tools: Vec::new(),
            params: vec![String::from("path")],
            category: String::from("sensitive_file_access"),
            severity: String::from("HIGH"),
            patterns: vec![String::from("all-tools-token")],
            exclude_patterns: Vec::new(),
            description: String::from("custom all-tools finding"),
            remediation: String::new(),
        });
        let policy = SecurityPolicy::new(settings).expect("all-tools policy");
        assert!(matches!(
            policy.evaluate(
                "read_file",
                r#"{"path":"all-tools-token"}"#,
                SecurityApprovalMode::Auto,
            ),
            ToolGuardEffect::Ask(_)
        ));
    }

    #[test]
    fn ip_hosts_are_normalized_and_validated() {
        assert_eq!(
            normalize_ip_hosts(&[
                String::from(" 127.0.0.1 "),
                String::new(),
                String::from("0:0:0:0:0:0:0:1"),
                String::from("127.0.0.1"),
            ]),
            Ok(vec![String::from("127.0.0.1"), String::from("::1")])
        );
        assert!(normalize_ip_hosts(&[String::from("localhost")]).is_err());
    }
}
