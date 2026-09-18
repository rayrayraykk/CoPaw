//! Category bindings and provider result normalization from the legacy market.

use super::*;

// id, English label, Chinese label, native QwenPaw category, native ModelScope category.
const CATEGORIES: [(&str, &str, &str, &str, Option<&str>); 13] = [
    ("app", "Apps", "应用", "app", None),
    (
        "engineering-development",
        "Engineering",
        "工程开发",
        "engineering development",
        Some("developer-tools"),
    ),
    (
        "data-research",
        "Data & Research",
        "数据研究",
        "data research",
        None,
    ),
    (
        "document-office",
        "Docs & Office",
        "文档办公",
        "document office",
        None,
    ),
    (
        "design-creation",
        "Design",
        "设计创作",
        "design creation",
        None,
    ),
    (
        "automation-integration",
        "Automation",
        "自动化集成",
        "automation integration",
        None,
    ),
    (
        "product-management",
        "Product",
        "产品管理",
        "product management",
        None,
    ),
    (
        "marketing-growth",
        "Marketing",
        "营销增长",
        "marketing growth",
        Some("marketing-seo"),
    ),
    (
        "security-compliance",
        "Security",
        "安全合规",
        "security compliance",
        None,
    ),
    (
        "education-knowledge",
        "Education",
        "教育知识",
        "education knowledge",
        None,
    ),
    (
        "plugin-development",
        "Plugin Dev",
        "Plugin 开发",
        "plugin development",
        None,
    ),
    (
        "skills-management",
        "Skills",
        "Skills 管理",
        "skills management",
        Some("skill-management"),
    ),
    ("others", "Others", "其它", "others", Some("other")),
];

fn chinese(lang: &str) -> bool {
    lang.to_ascii_lowercase().starts_with("zh")
}

pub(super) fn categories(lang: &str) -> Value {
    json!(
        CATEGORIES
            .iter()
            .map(
                |(id, en, zh, _, _)| json!({"id": id, "label": if chinese(lang) { zh } else { en }})
            )
            .collect::<Vec<_>>()
    )
}

pub(super) fn routing(
    category: Option<&str>,
    provider: &str,
    lang: &str,
) -> (Option<&'static str>, Option<&'static str>) {
    let Some((id, _, zh, code, modelscope)) = CATEGORIES
        .iter()
        .find(|(id, _, _, _, _)| Some(*id) == category)
    else {
        return (None, None);
    };
    let native = match provider {
        "qwenpaw" => Some(*code),
        "modelscope" => *modelscope,
        _ => None,
    };
    if native.is_some() {
        return (native, None);
    }
    (
        None,
        Some(if *id == "app" {
            if chinese(lang) {
                "应用 PawApp"
            } else {
                "app PawApp"
            }
        } else if chinese(lang) {
            zh
        } else {
            code
        }),
    )
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn field(item: &Value, key: &str) -> Option<String> {
    text(item.get(key))
}

pub(super) fn integer(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn localized(item: &Value, key: &str, lang: &str) -> Option<String> {
    let locales = item.get("locales");
    for lang in if chinese(lang) {
        ["zh", "en"]
    } else {
        ["en", "zh"]
    } {
        if let Some(value) = text(
            locales
                .and_then(|locales| locales.get(lang))
                .and_then(|locale| locale.get(key)),
        ) {
            return Some(value);
        }
    }
    field(item, key)
}

fn quoted_id(id: &str) -> String {
    id.split('/')
        .map(|part| signing::encode(part).replace("%40", "@"))
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn platform(item: &Value, source: &str, lang: &str) -> Option<ResultItem> {
    let slug = field(item, "id")?;
    let mut stats = BTreeMap::new();
    for (field, name) in [("downloads", "downloads"), ("view_count", "views")] {
        if let Some(value) = item.get(field).and_then(integer) {
            stats.insert(name.to_owned(), json!(value));
        }
    }
    if let Some(category) = localized(item, "category", lang) {
        stats.insert(String::from("category"), json!(category));
    }
    let author = field(item, "developer")
        .or_else(|| {
            if source == "modelscope" && slug.starts_with('@') {
                slug.split_once('/')
                    .map(|(owner, _)| owner.trim_start_matches('@').to_owned())
            } else {
                None
            }
        })
        .or_else(|| field(item, "owner"));
    let base = if source == "qwenpaw" {
        "https://platform.agentscope.io"
    } else {
        "https://modelscope.cn"
    };
    Some(ResultItem {
        source: source.to_owned(),
        name: field(item, "display_name").unwrap_or_else(|| slug.clone()),
        description: localized(item, "description", lang),
        source_url: format!("{base}/skills/{}", quoted_id(&slug)),
        version: field(item, "version"),
        author,
        icon_url: field(item, "logo_url"),
        stats: (!stats.is_empty()).then_some(stats),
        slug,
    })
}

pub(super) fn claw_search(item: &Value) -> Option<ResultItem> {
    let slug = field(item, "slug").or_else(|| field(item, "name"))?;
    let owner = item.get("owner");
    Some(ResultItem {
        source: String::from("clawhub"),
        name: field(item, "name")
            .or_else(|| field(item, "displayName"))
            .unwrap_or_else(|| slug.clone()),
        description: field(item, "description").or_else(|| field(item, "summary")),
        source_url: field(item, "url").unwrap_or_else(|| format!("https://clawhub.ai/{slug}")),
        version: field(item, "version"),
        author: owner
            .and_then(|owner| field(owner, "displayName"))
            .or_else(|| owner.and_then(|owner| field(owner, "handle")))
            .or_else(|| field(item, "ownerHandle")),
        icon_url: owner.and_then(|owner| field(owner, "image")),
        stats: None,
        slug,
    })
}

pub(super) fn claw_browse(item: &Value) -> Option<ResultItem> {
    let slug = field(item, "slug")?;
    let stats = ["downloads", "stars", "installs"]
        .into_iter()
        .filter_map(|key| {
            item.get("stats")?
                .get(key)?
                .as_i64()
                .map(|value| (key.to_owned(), json!(value)))
        })
        .collect::<BTreeMap<_, _>>();
    Some(ResultItem {
        source: String::from("clawhub"),
        name: field(item, "displayName").unwrap_or_else(|| slug.clone()),
        description: field(item, "summary").or_else(|| field(item, "description")),
        source_url: format!("https://clawhub.ai/{slug}"),
        version: text(item.pointer("/tags/latest")),
        author: None,
        icon_url: None,
        stats: (!stats.is_empty()).then_some(stats),
        slug,
    })
}

pub(super) fn aliyun(item: &Value) -> Option<ResultItem> {
    let slug = field(item, "skillName").or_else(|| field(item, "displayName"))?;
    let mut stats = BTreeMap::new();
    for (key, target) in [("installCount", "installs"), ("likeCount", "likes")] {
        if let Some(value) = item.get(key).and_then(integer) {
            stats.insert(target.to_owned(), json!(value));
        }
    }
    let category = [field(item, "categoryName"), field(item, "subCategoryName")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" / ");
    if !category.is_empty() {
        stats.insert(String::from("category"), json!(category));
    }
    if let Some(updated) = field(item, "updatedAt") {
        stats.insert(String::from("updated_at"), json!(updated));
    }
    Some(ResultItem {
        source: String::from("aliyun"),
        name: field(item, "displayName").unwrap_or_else(|| slug.clone()),
        description: field(item, "description"),
        source_url: format!(
            "https://api.aliyun.com/agentexplorer/skills/{}",
            signing::encode(&slug)
        ),
        version: None,
        author: None,
        icon_url: None,
        stats: (!stats.is_empty()).then_some(stats),
        slug,
    })
}
