//! Image generation providers for the Atria image studio and chat mode.
//!
//! Six backends behind one `generate()` entry point:
//!
//! * `hf`       — Hugging Face Inference API (OpenAI-compatible router;
//!                `Authorization: Bearer <HF token>`). Also tolerates the
//!                legacy serverless endpoint that returns raw image bytes.
//! * `openai`   — any OpenAI-compatible `/images/generate` endpoint.
//! * `google`   — Imagen through the Gemini API (`generateContent` with
//!                `responseModalities: ["IMAGE","TEXT"]`).
//! * `stability`— Stability AI `stable-image` core endpoint (base64 artifacts).
//! * `a1111`    — local Automatic1111 WebUI (`/sdapi/v1/txt2img`).
//! * `comfy`    — local ComfyUI (minimal txt2img workflow, poll `/history`,
//!                fetch outputs through `/view`).
//!
//! No secrets are persisted here — the UI passes the full source config per
//! call (same pattern as chat connections).

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const HF_ROUTER_DEFAULT: &str = "https://router.huggingface.co";
pub const OPENAI_DEFAULT: &str = "https://api.openai.com/v1";
pub const GOOGLE_API: &str = "https://generativelanguage.googleapis.com/v1beta/models";
pub const STABILITY_API: &str = "https://api.stability.ai/v2beta/stable-image/generate/core";
pub const A1111_DEFAULT: &str = "http://127.0.0.1:7860";
pub const COMFY_DEFAULT: &str = "http://127.0.0.1:8188";

pub const HF_MODEL_DEFAULT: &str = "stabilityai/stable-diffusion-xl-base-1.0";
pub const GOOGLE_MODEL_DEFAULT: &str = "gemini-2.5-flash-image";
pub const OPENAI_MODEL_DEFAULT: &str = "dall-e-3";

/// One configured image source.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageSource {
    Hf {
        token: String,
        #[serde(default)]
        base_url: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    Openai {
        key: String,
        #[serde(default)]
        base_url: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    Google {
        key: String,
        #[serde(default)]
        model: Option<String>,
    },
    Stability {
        key: String,
        #[serde(default)]
        model: Option<String>,
    },
    A1111 {
        #[serde(default)]
        url: Option<String>,
    },
    Comfy {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        checkpoint: Option<String>,
    },
}

impl ImageSource {
    pub fn kind(&self) -> &'static str {
        match self {
            ImageSource::Hf { .. } => "hf",
            ImageSource::Openai { .. } => "openai",
            ImageSource::Google { .. } => "google",
            ImageSource::Stability { .. } => "stability",
            ImageSource::A1111 { .. } => "a1111",
            ImageSource::Comfy { .. } => "comfy",
        }
    }

    fn model(&self) -> String {
        match self {
            ImageSource::Hf { model, .. } => model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| HF_MODEL_DEFAULT.to_string()),
            ImageSource::Openai { model, .. } => model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| OPENAI_MODEL_DEFAULT.to_string()),
            ImageSource::Google { model, .. } => model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| GOOGLE_MODEL_DEFAULT.to_string()),
            ImageSource::Stability { model, .. } => model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| "stable-image".to_string()),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRequest {
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    #[serde(default = "def_w")]
    pub width: u32,
    #[serde(default = "def_h")]
    pub height: u32,
    #[serde(default = "def_n")]
    pub n: u32,
    /// sampling steps (local backends; ignored by most cloud APIs)
    #[serde(default = "def_steps")]
    pub steps: u32,
}

fn def_w() -> u32 { 1024 }
fn def_h() -> u32 { 1024 }
fn def_n() -> u32 { 1 }
fn def_steps() -> u32 { 25 }

impl Default for ImageRequest {
    fn default() -> Self {
        Self { prompt: String::new(), negative: String::new(), width: 1024, height: 1024, n: 1, steps: 25 }
    }
}

impl ImageRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        let p = self.prompt.trim();
        if p.is_empty() {
            return Err("پرامپت خالی است".into());
        }
        if p.len() > 8000 {
            return Err("پرامپت بیش از حد طولانی است".into());
        }
        self.width = self.width.clamp(256, 2048);
        self.height = self.height.clamp(256, 2048);
        self.n = self.n.clamp(1, 4);
        self.steps = self.steps.clamp(1, 150);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageOut {
    /// image bytes, base64
    pub b64: String,
    pub mime: String,
    /// original byte length
    pub bytes: u64,
    pub source: String,
    pub model: String,
}

/// Generate `req.n` images from the given source.
pub async fn generate(source: &ImageSource, req: &ImageRequest) -> Result<Vec<ImageOut>, String> {
    match source {
        ImageSource::Hf { token, base_url, model: _ } => {
            let base = norm_base(base_url, HF_ROUTER_DEFAULT);
            let token = token.trim().to_string();
            if token.is_empty() {
                return Err("توکن Hugging Face لازم است (در بخش مدل‌ها ذخیره کنید)".into());
            }
            openai_images_call(&format!("{base}/v1/images/generations"), &token, source.model(), req, true)
                .await
        }
        ImageSource::Openai { key, base_url, model: _ } => {
            let base = norm_base(base_url, OPENAI_DEFAULT);
            let key = key.trim().to_string();
            if key.is_empty() {
                return Err("کلید API OpenAI خالی است".into());
            }
            openai_images_call(&format!("{base}/images/generate"), &key, source.model(), req, false)
                .await
        }
        ImageSource::Google { key, model: _ } => google_imagen(key, source.model(), req).await,
        ImageSource::Stability { key, model: _ } => stability_call(key, source.model(), req).await,
        ImageSource::A1111 { url } => a1111_call(norm_base(url, A1111_DEFAULT), req).await,
        ImageSource::Comfy { url, checkpoint } => {
            let ckpt = checkpoint.as_deref().unwrap_or("").trim();
            if ckpt.is_empty() {
                return Err("نام checkpoint ComfyUI را انتخاب کنید".into());
            }
            comfy_call(norm_base(url, COMFY_DEFAULT), ckpt, req).await
        }
    }
}

fn norm_base(raw: &Option<String>, default: &str) -> String {
    let s = raw.as_deref().unwrap_or(default).trim().trim_end_matches('/');
    if s.is_empty() {
        default.to_string()
    } else {
        s.to_string()
    }
}

fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("atria-desktop/0.24 (image studio)")
        .timeout(timeout)
        .build()
        .expect("reqwest client")
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Parse a base64 JSON field, tolerating data-URL prefixes.
fn decode_b64_field(raw: &str) -> Result<Vec<u8>, String> {
    let raw = raw.trim();
    let body = raw
        .strip_prefix("data:")
        .and_then(|r| r.split_once(','))
        .map(|(_, b)| b)
        .unwrap_or(raw);
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|e| format!("invalid base64 image: {e}"))
}

// ---------------------------------------------------------------------------
// OpenAI-compatible (also used for the HF router)
// ---------------------------------------------------------------------------

async fn openai_images_call(
    url: &str,
    token: &str,
    model: String,
    req: &ImageRequest,
    hf_router: bool,
) -> Result<Vec<ImageOut>, String> {
    let size = format!("{}x{}", req.width, req.height);
    let mut body = serde_json::json!({
        "model": model,
        "prompt": req.prompt,
        "n": req.n,
        "size": size,
    });
    // HF router accepts negative_prompt on SD-class models
    if hf_router && !req.negative.trim().is_empty() {
        body["negative_prompt"] = serde_json::json!(req.negative);
    }
    let resp = client(Duration::from_secs(600))
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    // Legacy HF serverless endpoint answers with raw image bytes.
    if ctype.starts_with("image/") {
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        return Ok(vec![ImageOut {
            b64: b64(&bytes),
            mime: ctype.split(';').next().unwrap_or("image/png").to_string(),
            bytes: bytes.len() as u64,
            source: "hf".into(),
            model,
        }]);
    }
    let text = if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        let hint = if t.len() > 400 { String::from(&t[..400]) } else { t };
        return Err(format!("HTTP {status} — {hint}"));
    } else {
        resp.text().await.map_err(|e| e.to_string())?
    };
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| "پاسخ سرور image معتبر نبود (JSON)".to_string())?;
    if let Some(err) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(err.to_string());
    }
    let data = v.get("data").and_then(|d| d.as_array())
        .ok_or_else(|| "no image data in response".to_string())?;
    let mut out = Vec::new();
    for item in data {
        let http = client(Duration::from_secs(120));
        if let Some(b64raw) = item.get("b64_json").and_then(|b| b.as_str()) {
            let bytes = decode_b64_field(b64raw)?;
            out.push(ImageOut { b64: b64(&bytes), mime: "image/png".into(), bytes: bytes.len() as u64, source: "hf/openai".into(), model: model.clone() });
        } else if let Some(u) = item.get("url").and_then(|u| u.as_str()) {
            let rb = http.get(u).send().await.map_err(|e| format!("image download failed: {e}"))?;
            let ok = rb.status().is_success();
            let status = rb.status();
            let mime = rb
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/png")
                .split(';')
                .next()
                .unwrap_or("image/png")
                .to_string();
            if !ok {
                return Err(format!("image download HTTP {status}"));
            }
            let bytes = rb.bytes().await.map_err(|e| e.to_string())?;
            out.push(ImageOut { b64: b64(&bytes), mime, bytes: bytes.len() as u64, source: "hf/openai".into(), model: model.clone() });
        }
    }
    if out.is_empty() {
        return Err("سرور image را برگرداند (data خالی)".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Google Imagen (via Gemini)
// ---------------------------------------------------------------------------

async fn google_imagen(key: &str, model: String, req: &ImageRequest) -> Result<Vec<ImageOut>, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("کلید Google API خالی است".into());
    }
    let url = format!("{GOOGLE_API}/{model}:generateContent");
    let body = serde_json::json!({
        "contents": [{ "role": "user", "parts": [{ "text": req.prompt }] }],
        "generationConfig": { "responseModalities": ["IMAGE", "TEXT"] }
    });
    let http = client(Duration::from_secs(600));
    let mut out = Vec::new();
    for _ in 0..req.n {
        let resp = http
            .post(&url)
            .header("x-goog-api-key", &key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("bad JSON: {e}"))?;
        if !status.is_success() {
            let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("unknown");
            return Err(format!("HTTP {status} — {msg}"));
        }
        let part = v
            .pointer("/candidates/0/content/parts")
            .and_then(|p| p.as_array())
            .ok_or_else(|| "no parts in Gemini response".to_string())?;
        let mut found = false;
        for p in part {
            if let Some(id) = p.get("inlineData").or_else(|| p.get("inline_data")) {
                let mime = id.get("mimeType").or_else(|| id.get("mime_type"))
                    .and_then(|m| m.as_str()).unwrap_or("image/png").to_string();
                let data = id.get("data").and_then(|d| d.as_str())
                    .ok_or_else(|| "no inline image data".to_string())?;
                let bytes = decode_b64_field(data)?;
                out.push(ImageOut { b64: b64(&bytes), mime, bytes: bytes.len() as u64, source: "google".into(), model: model.clone() });
                found = true;
                break;
            }
        }
        if !found {
            return Err("مدل image تولید نکرد (تنها متن). model یا quota را بررسی کنید".into());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Stability AI
// ---------------------------------------------------------------------------

async fn stability_call(key: &str, model: String, req: &ImageRequest) -> Result<Vec<ImageOut>, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("کلید Stability API خالی است".into());
    }
    let http = client(Duration::from_secs(600));
    let mut out = Vec::new();
    for _ in 0..req.n {
        let mut body = serde_json::json!({
            "prompt": req.prompt,
            "width": req.width,
            "height": req.height,
            "output_format": "png",
        });
        if !req.negative.trim().is_empty() {
            body["negative_prompt"] = serde_json::json!(req.negative);
        }
        let resp = http
            .post(STABILITY_API)
            .bearer_auth(&key)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("bad JSON: {e}"))?;
        if !status.is_success() {
            let msg = v.get("error").and_then(|m| m.as_str()).unwrap_or("unknown");
            return Err(format!("HTTP {status} — {msg}"));
        }
        let arts = v.get("artifacts").and_then(|a| a.as_array())
            .ok_or_else(|| "no artifacts in Stability response".to_string())?;
        if let Some(a) = arts.first() {
            let b64raw = a.get("base64").and_then(|b| b.as_str())
                .ok_or_else(|| "artifact بدون base64".to_string())?;
            let bytes = decode_b64_field(b64raw)?;
            out.push(ImageOut { b64: b64(&bytes), mime: "image/png".into(), bytes: bytes.len() as u64, source: "stability".into(), model: model.clone() });
        }
    }
    if out.is_empty() {
        return Err("Stability image برگرداند".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// A1111 WebUI (local)
// ---------------------------------------------------------------------------

async fn a1111_call(base: String, req: &ImageRequest) -> Result<Vec<ImageOut>, String> {
    let url = format!("{base}/sdapi/v1/txt2img");
    let body = serde_json::json!({
        "prompt": req.prompt,
        "negative_prompt": req.negative,
        "width": req.width,
        "height": req.height,
        "batch_size": req.n,
        "steps": req.steps,
        "seed": -1,
    });
    let resp = client(Duration::from_secs(1800))
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("اتصال به A1111 ({base}) ممکن نشد — مطمئن شوید WebUI اجراست: {e}"))?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("bad JSON from A1111: {e}"))?;
    if !status.is_success() {
        return Err(format!("A1111 HTTP {status}: {}", v.get("error").and_then(|m| m.as_str()).unwrap_or("unknown")));
    }
    let images = v.get("images").and_then(|i| i.as_array())
        .ok_or_else(|| "A1111 images برگرداند".to_string())?;
    let mut out = Vec::new();
    for raw in images {
        let bytes = decode_b64_field(raw.as_str().unwrap_or(""))?;
        out.push(ImageOut { b64: b64(&bytes), mime: "image/png".into(), bytes: bytes.len() as u64, source: "a1111".into(), model: "local".into() });
    }
    if out.is_empty() {
        return Err("A1111 عکسی تولید نکرد".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ComfyUI (local)
// ---------------------------------------------------------------------------

fn comfy_workflow(prompt: &str, negative: &str, width: u32, height: u32, n: u32, steps: u32, ckpt: &str) -> serde_json::Value {
    serde_json::json!({
        "11": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": ckpt } },
        "6":  { "class_type": "CLIPTextEncode", "inputs": { "text": prompt, "clip": ["11", 1] } },
        "7":  { "class_type": "CLIPTextEncode", "inputs": { "text": negative, "clip": ["11", 1] } },
        "9":  { "class_type": "EmptyLatentImage", "inputs": { "width": width, "height": height, "batch_size": n } },
        "3":  { "class_type": "KSampler", "inputs": {
            "seed": rand_seed(), "steps": steps, "cfg": 7.0,
            "sampler_name": "euler_ancestral", "scheduler": "normal", "denoise": 1.0,
            "model": ["11", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["9", 0]
        } },
        "4":  { "class_type": "VAEDecode", "inputs": { "samples": ["3", 0], "vae": ["11", 2] } },
        "5":  { "class_type": "SaveImage", "inputs": { "filename_prefix": "atria", "images": ["4", 0] } }
    })
}

fn rand_seed() -> u64 {
    // time-based seed is fine for a desktop app
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64 % 4_294_967_296)
        .unwrap_or(1)
}

async fn comfy_call(base: String, ckpt: &str, req: &ImageRequest) -> Result<Vec<ImageOut>, String> {
    let http = client(Duration::from_secs(3600));
    let wf = comfy_workflow(&req.prompt, &req.negative, req.width, req.height, req.n, req.steps, ckpt);
    let resp = http
        .post(format!("{base}/prompt"))
        .json(&serde_json::json!({ "client_id": "atria-desktop", "prompt": wf }))
        .send()
        .await
        .map_err(|e| format!("اتصال به ComfyUI ({base}) ممکن نشد — مطمئن شوید اجراست: {e}"))?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("bad JSON from ComfyUI: {e}"))?;
    if !status.is_success() {
        return Err(format!("ComfyUI HTTP {status}: {v}"));
    }
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(format!("workflow رد شد: {err}"));
    }
    let prompt_id = v.get("prompt_id").and_then(|p| p.as_str())
        .ok_or_else(|| "ComfyUI prompt_id نداد".to_string())?;

    // poll history up to 10 minutes
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    let entry: serde_json::Value = loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match http.get(format!("{base}/history/{prompt_id}")).send().await {
            Ok(r) => match r.json::<serde_json::Value>().await {
                Ok(h) => {
                    if let Some(e) = h.get(prompt_id) {
                        break e.clone();
                    }
                }
                Err(_) => {}
            },
            Err(_) => {}
        }
        if std::time::Instant::now() > deadline {
            return Err("ComfyUI بعد از ۱۰ دقیقه جواب نداد (هنوز در صف است؟)".into());
        }
    };
    if let Some(errs) = entry.get("status").and_then(|s| s.get("status_str")).and_then(|s| s.as_str()) {
        if errs == "error" {
            let detail = entry.pointer("/status/messages").cloned().unwrap_or_default();
            return Err(format!("ComfyUI execution error: {detail}"));
        }
    }
    let outputs = entry.get("outputs").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
    for (node, ov) in outputs.as_object().into_iter().flatten() {
        let _ = node;
        for img in ov.get("images").and_then(|i| i.as_array()).into_iter().flatten() {
            let filename = img.get("filename").and_then(|f| f.as_str()).unwrap_or_default();
            let subfolder = img.get("subfolder").and_then(|f| f.as_str()).unwrap_or_default();
            let itype = img.get("type").and_then(|f| f.as_str()).unwrap_or("output");
            if filename.is_empty() {
                continue;
            }
            let view = format!(
                "{base}/view?filename={}&subfolder={}&type={}",
                urlenc_q(filename),
                urlenc_q(subfolder),
                urlenc_q(itype)
            );
            let rb = http.get(&view).send().await.map_err(|e| format!("fetch image: {e}"))?;
            let ok = rb.status().is_success();
            let status = rb.status();
            let mime = rb
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/png")
                .split(';')
                .next()
                .unwrap_or("image/png")
                .to_string();
            if !ok {
                return Err(format!("ComfyUI /view HTTP {status}"));
            }
            let bytes = rb.bytes().await.map_err(|e| e.to_string())?;
            let out = ImageOut { b64: b64(&bytes), mime, bytes: bytes.len() as u64, source: "comfy".into(), model: ckpt.to_string() };
            return Ok(vec![out]); // one image per queue item is enough for v1
        }
    }
    Err("ComfyUI عکسی در خروجی پیدا نکرد (SaveImage گم است؟)".into())
}

fn urlenc_q(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn src(kind: &str) -> ImageSource {
        serde_json::from_value(match kind {
            "hf" => serde_json::json!({"kind": "hf", "token": "hf_x"}),
            "openai" => serde_json::json!({"kind": "openai", "key": "sk-x"}),
            "google" => serde_json::json!({"kind": "google", "key": "g-x"}),
            "stability" => serde_json::json!({"kind": "stability", "key": "st-x"}),
            "a1111" => serde_json::json!({"kind": "a1111"}),
            "comfy" => serde_json::json!({"kind": "comfy", "checkpoint": "sd_xl.safetensors"}),
            _ => panic!("{kind}"),
        }).unwrap()
    }

    #[test]
    fn source_kinds_and_defaults() {
        assert_eq!(src("hf").kind(), "hf");
        assert_eq!(src("hf").model(), HF_MODEL_DEFAULT);
        assert_eq!(src("openai").model(), OPENAI_MODEL_DEFAULT);
        assert_eq!(src("google").model(), GOOGLE_MODEL_DEFAULT);
        assert_eq!(norm_base(&None, "https://x/y"), "https://x/y");
        assert_eq!(norm_base(&Some("https://a/b/".into()), "d"), "https://a/b");
        assert_eq!(norm_base(&Some(" ".into()), "d"), "d");
    }

    #[test]
    fn request_validation_bounds() {
        let mut r = ImageRequest { prompt: "  ".into(), ..Default::default() };
        assert!(r.validate().is_err());
        let mut r = ImageRequest { prompt: "cat".into(), negative: String::new(), width: 64, height: 99999, n: 99, steps: 0 };
        r.validate().unwrap();
        assert_eq!(r.width, 256);
        assert_eq!(r.height, 2048);
        assert_eq!(r.n, 4);
        assert_eq!(r.steps, 1);
    }

    #[test]
    fn b64_roundtrip_and_data_url() {
        let bytes = vec![1, 2, 3, 250];
        let enc = b64(&bytes);
        let dec = decode_b64_field(&enc).unwrap();
        assert_eq!(dec, bytes);
        let du = format!("data:image/png;base64,{enc}");
        assert_eq!(decode_b64_field(&du).unwrap(), bytes);
        assert!(decode_b64_field("!!not-b64!!").is_err());
    }

    #[test]
    fn comfy_workflow_shape() {
        let wf = comfy_workflow("a cat", "blur", 1024, 768, 2, 30, "ckpt.safetensors");
        let obj = wf.as_object().unwrap();
        assert_eq!(obj.len(), 7);
        assert_eq!(wf["11"]["inputs"]["ckpt_name"], "ckpt.safetensors");
        assert_eq!(wf["9"]["inputs"]["batch_size"], 2);
        assert_eq!(wf["3"]["inputs"]["steps"], 30);
        assert_eq!(wf["3"]["inputs"]["positive"][0], "6");
        assert_eq!(wf["5"]["class_type"], "SaveImage");
    }

    #[test]
    fn google_payload_shape() {
        let body = serde_json::json!({
            "contents": [{ "role": "user", "parts": [{ "text": "sunset" }] }],
            "generationConfig": { "responseModalities": ["IMAGE", "TEXT"] }
        });
        assert_eq!(body["generationConfig"]["responseModalities"][0], "IMAGE");
    }

    /// Live generation against a configured provider (network). Ignored.
    ///
    /// ```sh
    /// ATRIA_IMG_KIND=hf ATRIA_HF_TOKEN=... cargo test -p atria-core --lib imggen::tests::live_generate -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "hits the live image API"]
    async fn live_generate() {
        let kind = std::env::var("ATRIA_IMG_KIND").unwrap_or_else(|_| "hf".into());
        let token = std::env::var(if kind == "hf" { "ATRIA_HF_TOKEN" } else { "ATRIA_API_KEY" }).unwrap_or_default();
        let source: ImageSource = serde_json::from_value(
            serde_json::json!({"kind": kind, "token": token, "key": token}),
        ).unwrap();
        let req = ImageRequest { prompt: "a red cube on a white background".into(), ..Default::default() };
        let outs = generate(&source, &req).await.expect("generation");
        assert!(!outs.is_empty());
        assert!(!outs[0].b64.is_empty());
    }
}
