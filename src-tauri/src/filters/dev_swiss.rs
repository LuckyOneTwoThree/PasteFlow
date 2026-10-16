//! L1 滤镜【开发者瑞士军刀】(Master Spec §10.2 #6)
//!
//! JSON 格式化与转 TS/Go/Python/Rust 类型定义；cURL 转 `fetch`；
//! 时间戳、JWT 与 Base64 解码。**仅通过 HUD / 菜单手动触发**（`MANUAL_ONLY`），
//! 输出 `PASSTHROUGH` 通道策略。
//!
//! TDD §7.4: 预编译 + serde_json 确定性解析, 零网络调用。

use base64::Engine;
use std::sync::OnceLock;

// —— 检测与分发 ——

/// 可识别的开发者载荷类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevPayloadKind {
    Json,
    Curl,
    UnixTimestamp,
    Jwt,
    Base64,
    Unknown,
}

/// 自动检测载荷类型（HUD 卡片标题用）
pub fn detect_kind(source: &str) -> DevPayloadKind {
    let s = source.trim();
    if s.starts_with("curl ") || s.starts_with("curl\t") {
        return DevPayloadKind::Curl;
    }
    if s.split('.').count() == 3 && s.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        && looks_base64url(s.split('.').next().unwrap_or(""))
    {
        return DevPayloadKind::Jwt;
    }
    if (s.starts_with('{') && s.ends_with('}')) || (s.starts_with('[') && s.ends_with(']')) {
        if serde_json::from_str::<serde_json::Value>(s).is_ok() {
            return DevPayloadKind::Json;
        }
        return DevPayloadKind::Unknown;
    }
    if s.len() == 10 && s.chars().all(|c| c.is_ascii_digit()) {
        return DevPayloadKind::UnixTimestamp;
    }
    if looks_base64(s) && s.len() >= 8 {
        return DevPayloadKind::Base64;
    }
    DevPayloadKind::Unknown
}

fn looks_base64url(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn looks_base64(s: &str) -> bool {
    let stripped: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    stripped.len() % 4 == 0
        && stripped.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='
        })
}

// —— JSON ——

/// JSON 格式化（2 空格缩进）
pub fn format_json(source: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(source.trim()).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// JSON → TypeScript interface
pub fn json_to_ts(source: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(source.trim()).map_err(|e| e.to_string())?;
    let mut out = String::from("interface Root {\n");
    write_fields(&v, 1, &mut out, "ts");
    out.push('}');
    Ok(out)
}

/// JSON → Go struct
pub fn json_to_go(source: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(source.trim()).map_err(|e| e.to_string())?;
    let mut out = String::from("type Root struct {\n");
    write_fields(&v, 1, &mut out, "go");
    out.push('}');
    Ok(out)
}

/// JSON → Pydantic 类定义
pub fn json_to_python(source: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(source.trim()).map_err(|e| e.to_string())?;
    let mut out = String::from("class Root(BaseModel):\n");
    if v.as_object().map(|o| o.is_empty()).unwrap_or(false) {
        out.push_str("    pass");
        return Ok(out);
    }
    write_fields(&v, 1, &mut out, "py");
    Ok(out)
}

/// JSON → Rust struct
pub fn json_to_rust(source: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(source.trim()).map_err(|e| e.to_string())?;
    let mut out = String::from("#[derive(Debug, Deserialize)]\nstruct Root {\n");
    write_fields(&v, 1, &mut out, "rs");
    out.push('}');
    Ok(out)
}

fn indent(level: usize) -> String {
    "    ".repeat(level)
}

fn type_of(v: &serde_json::Value, lang: &str) -> String {
    match v {
        serde_json::Value::Null => "any".to_string(),
        serde_json::Value::Bool(_) => {
            if lang == "go" {
                "bool".into()
            } else if lang == "py" {
                "bool".into()
            } else if lang == "rs" {
                "bool".into()
            } else {
                "boolean".into()
            }
        }
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                match lang {
                    "go" => "int64".into(),
                    "py" => "int".into(),
                    "rs" => "i64".into(),
                    _ => "number".into(),
                }
            } else {
                match lang {
                    "go" => "float64".into(),
                    "py" => "float".into(),
                    "rs" => "f64".into(),
                    _ => "number".into(),
                }
            }
        }
        serde_json::Value::String(_) => {
            match lang {
                "go" => "string".into(),
                "py" => "str".into(),
                "rs" => "String".into(),
                _ => "string".into(),
            }
        }
        serde_json::Value::Array(_) => {
            let inner = "any".to_string();
            match lang {
                "go" => format!("[]{inner}"),
                "py" => format!("list[{inner}]"),
                "rs" => format!("Vec<{inner}>"),
                _ => format!("{inner}[]"),
            }
        }
        serde_json::Value::Object(_) => match lang {
            "go" => "map[string]any".into(),
            "py" => "dict[str, Any]".into(),
            "rs" => "serde_json::Map<String, serde_json::Value>".into(),
            _ => "Record<string, any>".into(),
        },
    }
}

fn write_fields(v: &serde_json::Value, level: usize, out: &mut String, lang: &str) {
    let Some(obj) = v.as_object() else {
        return;
    };
    let ind = indent(level);
    for (k, val) in obj {
        let ty = type_of(val, lang);
        match lang {
            "go" => {
                // Go 导出字段: PascalCase
                let field = to_pascal(k);
                out.push_str(&format!("{ind}{field} {ty} `json:\"{k}\"`\n"));
            }
            "py" => out.push_str(&format!("{ind}{k}: {ty}\n")),
            "rs" => {
                let field = to_snake(k);
                out.push_str(&format!("{ind}{field}: {ty},\n"));
            }
            _ => out.push_str(&format!("{ind}{k}: {ty};\n")),
        }
    }
}

fn to_pascal(s: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        out.push_str("Field");
    }
    out
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c == '-' {
            out.push('_');
        } else if c.is_uppercase() {
            out.push('_');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert(0, '_');
    }
    out
}

// —— cURL → fetch ——

/// cURL 命令转 fetch（仅手动触发）
pub fn curl_to_fetch(source: &str) -> Result<String, String> {
    let s = source.trim();
    if !s.starts_with("curl ") {
        return Err("not a curl command".into());
    }
    // URL: 第一个 http(s) token 或 -X 之后
    let url_re = regex::Regex::new(r#"https?://[^\s"']+"#).expect("static regex");
    let url = url_re
        .find(s)
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| "no URL found in curl command".to_string())?;
    // method
    let method_re = regex::Regex::new(r#"-X\s+(\w+)"#).expect("static regex");
    let method = method_re
        .captures(s)
        .map(|c| c[1].to_uppercase())
        .unwrap_or_else(|| {
            if s.contains(" --data") || s.contains(" -d ") || s.contains("--data-raw") {
                "POST".to_string()
            } else {
                "GET".to_string()
            }
        });
    // headers: -H 'k: v'
    let header_re =
        regex::Regex::new(r#"-H\s+["']([^"']+)["']"#).expect("static regex");
    let headers: Vec<String> = header_re
        .captures_iter(s)
        .map(|c| format!("    '{}',", c[1].replace('\'', "\\'")))
        .collect();
    // body: --data-raw / -d
    let body_re =
        regex::Regex::new(r#"(?:--data-raw|--data|-d)\s+["']([^"']*)["']"#).expect("static regex");
    let body = body_re.captures(s).map(|c| c[1].to_string());

    let mut out = format!(
        "const res = await fetch('{}', {{\n  method: '{}',\n",
        url, method
    );
    if !headers.is_empty() {
        out.push_str("  headers: [\n");
        for h in &headers {
            out.push_str(h);
            out.push('\n');
        }
        out.push_str("  ].reduce((acc, h) => { const [k, v] = h.split(': '); acc[k] = v; return acc; }, {}),\n");
    }
    if let Some(b) = body {
        out.push_str(&format!("  body: JSON.stringify({}),\n", b.trim()));
    }
    out.push_str("});\nconst data = await res.json();");
    Ok(out)
}

// —— 解码 ——

/// Unix 时间戳 → ISO 8601 (UTC)
pub fn decode_timestamp(source: &str) -> Result<String, String> {
    let t: i64 = source.trim().parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    // 秒级 / 毫秒级自适应
    let secs = if t > 1_000_000_000_000 { t / 1000 } else { t };
    let datetime = time::OffsetDateTime::from_unix_timestamp(secs).map_err(|e| e.to_string())?;
    Ok(datetime
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| e.to_string())?)
}

/// JWT 解码（header + payload Base64Url，**不验签**）
pub fn decode_jwt(source: &str) -> Result<String, String> {
    let parts: Vec<&str> = source.trim().split('.').collect();
    if parts.len() != 3 {
        return Err("invalid JWT: expected 3 segments".into());
    }
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = b64.decode(parts[0]).map_err(|e| e.to_string())?;
    let payload = b64.decode(parts[1]).map_err(|e| e.to_string())?;
    let header_pretty = format_json(&String::from_utf8_lossy(&header))?;
    let payload_pretty = format_json(&String::from_utf8_lossy(&payload))?;
    Ok(format!(
        "// header\n{header_pretty}\n\n// payload (signature NOT verified)\n{payload_pretty}"
    ))
}

/// Base64 解码（UTF-8 优先，失败回退 hex 摘要）
pub fn decode_base64(source: &str) -> Result<String, String> {
    let stripped: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    let b64 = base64::engine::general_purpose::STANDARD;
    let raw = b64.decode(&stripped).map_err(|e| e.to_string())?;
    match String::from_utf8(raw.clone()) {
        Ok(text) => Ok(text),
        Err(_) => Ok(format!("(binary, {} bytes)\nhex: {}", raw.len(), hex_encode(&raw))),
    }
}

fn hex_encode(raw: &[u8]) -> String {
    static HEX: OnceLock<[u8; 16]> = OnceLock::new();
    let table = HEX.get_or_init(|| *b"0123456789abcdef");
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw {
        out.push(table[(b >> 4) as usize] as char);
        out.push(table[(b & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_json_and_curl() {
        assert_eq!(detect_kind(r#"{"a":1}"#), DevPayloadKind::Json);
        assert_eq!(detect_kind("curl https://x.com"), DevPayloadKind::Curl);
        assert_eq!(detect_kind("1729800000"), DevPayloadKind::UnixTimestamp);
        assert_eq!(detect_kind("hello world"), DevPayloadKind::Unknown);
    }

    #[test]
    fn format_json_pretty() {
        let s = format_json(r#"{"a":1,"b":[2]}"#).unwrap();
        assert!(s.contains("\"a\": 1"));
        assert!(s.contains('\n'));
    }

    #[test]
    fn json_to_ts_types() {
        let s = json_to_ts(r#"{"name":"x","age":3,"ok":true,"tags":["a"]}"#).unwrap();
        assert!(s.contains("name: string;"));
        assert!(s.contains("age: number;"));
        assert!(s.contains("ok: boolean;"));
        assert!(s.contains("tags: any[];"));
    }

    #[test]
    fn json_to_go_exported_fields() {
        let s = json_to_go(r#"{"user_name":"x"}"#).unwrap();
        assert!(s.contains("UserName string `json:\"user_name\"`"));
    }

    #[test]
    fn json_to_python_and_rust() {
        let py = json_to_python(r#"{"count":1}"#).unwrap();
        assert!(py.contains("count: int"));
        let rs = json_to_rust(r#"{"total":2}"#).unwrap();
        assert!(rs.contains("total: i64,"));
    }

    #[test]
    fn curl_to_fetch_basic() {
        let s = curl_to_fetch(
            r#"curl -X POST https://api.x.com/v1 -H 'Content-Type: application/json' --data-raw '{"a":1}'"#,
        )
        .unwrap();
        assert!(s.contains("fetch('https://api.x.com/v1'"));
        assert!(s.contains("method: 'POST'"));
        assert!(s.contains("Content-Type"));
    }

    #[test]
    fn curl_without_url_fails() {
        assert!(curl_to_fetch("curl --help").is_err());
    }

    #[test]
    fn decode_timestamp_seconds_and_ms() {
        assert_eq!(decode_timestamp("1729800000").unwrap(), "2024-10-24T20:00:00Z");
        assert_eq!(decode_timestamp("1729800000000").unwrap(), "2024-10-24T20:00:00Z");
    }

    #[test]
    fn decode_jwt_no_verify() {
        // header={"alg":"none"} payload={"sub":"123"} (unsigned demo)
        let jwt = "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxMjMifQ.sig";
        let s = decode_jwt(jwt).unwrap();
        assert!(s.contains("\"sub\": \"123\""));
        assert!(s.contains("NOT verified"));
    }

    #[test]
    fn decode_base64_utf8_and_binary() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), "hello");
        let s = decode_base64("//8=").unwrap(); // 0xFF 0xFF
        assert!(s.contains("binary"));
        assert!(s.contains("hex: ffff"));
    }

    #[test]
    fn detect_jwt_kind() {
        assert_eq!(detect_kind("eyJhbGciOiJub25lIn0.eyJzdWIiOiIxMjMifQ.sig"), DevPayloadKind::Jwt);
    }
}
