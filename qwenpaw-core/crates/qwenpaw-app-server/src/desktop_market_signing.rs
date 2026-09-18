//! Native ACS3-HMAC-SHA256 requests; no Python SDK or credential serialization.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::*;

pub(super) fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~")
}

pub(super) fn request(
    sources: &Sources,
    environment: &BTreeMap<String, String>,
    action: &str,
    path: &str,
    query: &BTreeMap<String, String>,
) -> Result<(url::Url, HeaderMap), String> {
    let base = environment
        .get("ALIYUN_AGENTEXPLORER_ENDPOINT")
        .filter(|value| !value.is_empty())
        .map_or_else(|| sources.aliyun.clone(), |host| format!("https://{host}"));
    let mut url = endpoint(&base, path)?;
    if url.scheme() != "https"
        && !url
            .host_str()
            .is_some_and(|host| matches!(host, "127.0.0.1" | "[::1]" | "localhost"))
    {
        return Err(String::from("Aliyun credentials require an HTTPS endpoint"));
    }
    let query = query
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    if !query.is_empty() {
        url.set_query(Some(&query));
    }
    let mut headers = BTreeMap::from([
        (
            String::from("host"),
            url[url::Position::BeforeHost..url::Position::AfterPort].to_owned(),
        ),
        (String::from("x-acs-action"), action.to_owned()),
        (String::from("x-acs-version"), String::from("2026-03-17")),
        (
            String::from("x-acs-date"),
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
        (
            String::from("x-acs-signature-nonce"),
            uuid::Uuid::now_v7().simple().to_string(),
        ),
        (
            String::from("x-acs-content-sha256"),
            format!("{:x}", Sha256::digest([])),
        ),
    ]);
    if let Some(token) = environment
        .get("ALIBABA_CLOUD_SECURITY_TOKEN")
        .filter(|token| !token.is_empty())
    {
        headers.insert(String::from("x-acs-security-token"), token.clone());
    }
    let id = environment
        .get("ALIBABA_CLOUD_ACCESS_KEY_ID")
        .ok_or("Aliyun access key is unavailable")?;
    let secret = environment
        .get("ALIBABA_CLOUD_ACCESS_KEY_SECRET")
        .ok_or("Aliyun access key is unavailable")?;
    let authorization = authorization("GET", &url, &headers, id, secret)?;
    headers.insert(String::from("authorization"), authorization);
    let mut result = HeaderMap::new();
    for (key, value) in headers {
        let mut value = value
            .parse::<axum::http::HeaderValue>()
            .map_err(|_| "Aliyun credential or header is invalid")?;
        if matches!(key.as_str(), "authorization" | "x-acs-security-token") {
            value.set_sensitive(true);
        }
        result.insert(
            key.parse::<axum::http::HeaderName>()
                .map_err(|_| "Aliyun header is invalid")?,
            value,
        );
    }
    Ok((url, result))
}

fn authorization(
    method: &str,
    url: &url::Url,
    headers: &BTreeMap<String, String>,
    id: &str,
    secret: &str,
) -> Result<String, String> {
    let names = headers
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(";");
    let mut canonical_headers = String::new();
    for (name, value) in headers {
        writeln!(canonical_headers, "{name}:{}", value.trim()).expect("writing to String");
    }
    let canonical = format!(
        "{method}\n{}\n{}\n{canonical_headers}\n{names}\n{}",
        url.path(),
        url.query().unwrap_or(""),
        headers["x-acs-content-sha256"]
    );
    let to_sign = format!(
        "ACS3-HMAC-SHA256\n{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    let mut hmac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| "Aliyun signing failed")?;
    hmac.update(to_sign.as_bytes());
    let mut signature = String::new();
    for byte in hmac.finalize().into_bytes() {
        write!(signature, "{byte:02x}").expect("writing to String");
    }
    Ok(format!(
        "ACS3-HMAC-SHA256 Credential={id},SignedHeaders={names},Signature={signature}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_official_acs3_signature_vector() {
        let url = url::Url::parse("https://ecs.cn-shanghai.aliyuncs.com/?ImageId=win2019_1809_x64_dtc_zh-cn_40G_alibase_20230811.vhd&RegionId=cn-shanghai").unwrap();
        let headers = BTreeMap::from([
            (
                String::from("host"),
                String::from("ecs.cn-shanghai.aliyuncs.com"),
            ),
            (String::from("x-acs-action"), String::from("RunInstances")),
            (String::from("x-acs-version"), String::from("2014-05-26")),
            (
                String::from("x-acs-date"),
                String::from("2023-10-26T10:22:32Z"),
            ),
            (
                String::from("x-acs-signature-nonce"),
                String::from("3156853299f313e23d1673dc12e1703d"),
            ),
            (
                String::from("x-acs-content-sha256"),
                format!("{:x}", Sha256::digest([])),
            ),
        ]);
        assert_eq!(
            authorization(
                "POST",
                &url,
                &headers,
                "YourAccessKeyId",
                "YourAccessKeySecret"
            )
            .unwrap(),
            "ACS3-HMAC-SHA256 Credential=YourAccessKeyId,SignedHeaders=host;x-acs-action;x-acs-content-sha256;x-acs-date;x-acs-signature-nonce;x-acs-version,Signature=06563a9e1b43f5dfe96b81484da74bceab24a1d853912eee15083a6f0f3283c0"
        );
        assert_eq!(encode(" ~*+/中"), "%20~%2A%2B%2F%E4%B8%AD");
        let environment = BTreeMap::from([
            (
                String::from("ALIBABA_CLOUD_ACCESS_KEY_ID"),
                String::from("fixture-id"),
            ),
            (
                String::from("ALIBABA_CLOUD_ACCESS_KEY_SECRET"),
                String::from("fixture-secret"),
            ),
            (
                String::from("ALIBABA_CLOUD_SECURITY_TOKEN"),
                String::from("fixture-token"),
            ),
        ]);
        let (_, headers) = request(
            &Sources::default(),
            &environment,
            "SearchSkills",
            "/openapi/skills",
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(headers["authorization"].is_sensitive());
        assert!(headers["x-acs-security-token"].is_sensitive());
        let diagnostic = format!("{headers:?}");
        assert!(!diagnostic.contains("fixture-token"));
        assert!(!diagnostic.contains("fixture-id"));
        assert!(!diagnostic.contains("fixture-secret"));
    }
}
