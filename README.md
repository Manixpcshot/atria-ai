# آتریا — دستیار هوشمند دسکتاپ 🌅

**Atria Dawn** یک اپ دسکتاپ ویندوز برای چت و کار با هوش مصنوعی **آتریا** (مدل `Atria-Dawn-Preview`) است —
ساخته‌شده با **موتور Rust** و پوستهٔ **Tauri v2**، با طراحی انیمیشنی «طلوع» (Dawn).

![Atria Dawn UI](docs/screen-chat.png)

## ✨ امکانات

- 🤖 **ایجنت واقعی** — حلقهٔ agent با ابزارهای داخلی (مثل ایجنت‌های حرفه‌ای):
  - 🧮 **ماشین‌حساب** — محاسبات دقیق ریاضی
  - ⏰ **ساعت و تاریخ** — زمان UTC و تهران
  - 🧠 **حافظه** — `remember` / `recall` برای یادآوری چیزها بین گفتگوها
- 🌊 **پاسخ زنده (Streaming)** — تایپ پاسخ لحظه‌به‌لحظه با SSE
- 💭 **نمایش تفکر** — بلوک‌های reasoning مدل (Atria-Dawn یک مدل Reasoning است)
- 🛠 **نمایش ابزارها** — کارت‌های انیمیشنی برای هر فراخوانی ابزار
- 💬 **گفتگوهای نامحدود** — ذخیرهٔ خودکار، جابه‌جایی سریع
- 🎨 **طراحی Dawn** — پس‌زمینهٔ شفقی متحرک، شیشه‌ای (glassmorphism)، انیمیشن‌های روان
- 📝 **مارک‌داون** — کد، لیست، نقل‌قول + دکمهٔ کپی
- 🖥 **پنجرهٔ بدون قاب** — مینیمایز/بیشینه/بستن سفارشی، گوشه‌های گرد
- 🇮🇷 **رابط کاملاً فارسی/راست‌چین** با فونت وزیرمی‌دان

## 📥 دانلود و نصب

از بخش [**Releases**](../../releases) آخرین نسخه را بگیرید:

| فایل | توضیح |
|------|--------|
| `Atria.exe` | نسخهٔ قابل‌اجرا (پرتابل) — کافی است اجرا کنید |
| `Atria-Dawn-win64.zip` | همان نسخه + README + LICENSE |

> نیازمندی: ویندوز ۱۰/۱۱ با **WebView2 Runtime** (روی ویندوز ۱۱ از پیش نصب است؛ [دانلود از مایکروسافت](https://developer.microsoft.com/microsoft-edge/webview2/) در صورت نیاز).

## ⚙️ تنظیم کلید API

۱. اپ را اجرا کنید → روی **تنظیمات** ⚙️ بزنید
۲. کلید Atria خود را (با پیشوند `atr_`) وارد کنید
۳. مدل به‌صورت پیش‌فرض `Atria-Dawn-Preview` است

> 🔑 کلید پیش‌فرض داخل مخزن گذاشته شده تا اپ «از جعبه» کار کند؛ بهتر است کلید شخصی خودتان را در تنظیمات قرار دهید.

## 🛠 ساخت از سورس

```bash
# نیازمندی: Rust (rustup.rs) + WebView2 Runtime
git clone https://github.com/Manixpcshot/atria-ai.git
cd atria-desktop

# اجرای نسخهٔ دیباگ
cargo run -p atria-app

# ساخت نسخهٔ ریلیز (خروجی: target/release/atria.exe)
cargo build --release -p atria-app

# تست‌های موتور
cargo test -p atria-core
```

ساخت خودکار ویندوز با **GitHub Actions** هم انجام می‌شود:
با هر بار انتشار Release (تگ `v*`)، ورک‌فلو [`.github/workflows/windows-release.yml`](.github/workflows/windows-release.yml)
یک `Atria.exe` می‌سازد و به همان Release ضمیمه می‌کند.

## 🧱 معماری

```
atria-desktop/
├── crates/
│   ├── atria-core/      # 🦀 موتور Rust: کلاینت Messages API، SSE، حلقهٔ ایجنت، ابزارها
│   │   ├── src/client.rs    # کلاینت سازگار با Anthropic Messages API
│   │   ├── src/sse.rs       # دیکدر جریان SSE
│   │   ├── src/agent.rs     # حلقهٔ ایجنت (مدل ⇄ ابزار)
│   │   ├── src/tools.rs     # ابزارها + ماشین‌حساب امن
│   │   └── src/memory.rs    # حافظهٔ پایدار remember/recall
│   └── atria-app/       # 🖥 پوستهٔ Tauri v2 + پنجرهٔ سفارشی
├── frontend/            # 🎨 UI انیمیشنی (HTML/CSS/JS + وزیرمی‌دان)
└── .github/workflows/   # ساخت خودکار ویندوز → ضمیمه به Release
```

### API

سازگار با **Anthropic Messages API**:

```bash
curl -X POST https://api.atria-asi.ai/v1/messages \
  -H "x-api-key: $ATRIA_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{"model": "Atria-Dawn-Preview", "max_tokens": 1024,
       "messages": [{"role": "user", "content": "hi"}]}'
```

موتور از **Streaming (SSE)** و **Tool Use** (فراخوانی ابزار) پشتیبانی می‌کند.

## 📄 لایسنس

[MIT](LICENSE)
