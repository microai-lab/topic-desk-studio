//! Desktop adapter for an isolated article webview embedded in the main window.

use crate::browser_profile::{self, BrowserProfile};
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg(target_os = "macos")]
use std::collections::HashSet;
use tauri::{
    webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder},
    Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};

#[cfg(target_os = "macos")]
use objc2::{
    define_class,
    ffi::{objc_setAssociatedObject, OBJC_ASSOCIATION_RETAIN_NONATOMIC},
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, NSObject, Sel},
    DefinedClass, MainThreadMarker, MainThreadOnly,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSData, NSObjectProtocol};
#[cfg(target_os = "macos")]
use objc2_web_kit::{WKDownload, WKWebViewConfiguration, WKWebsiteDataStore};
#[cfg(target_os = "macos")]
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
thread_local! {
    /// Wry calls the public Tauri download handler synchronously from these
    /// delegate methods, so a thread-local safely joins its opaque event to the
    /// native `WKDownload` without exposing a pointer outside the desktop adapter.
    static CURRENT_DOWNLOAD_START: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static CURRENT_DOWNLOAD_FINISH: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

#[cfg(target_os = "macos")]
fn native_download_handles() -> &'static Mutex<HashMap<i64, usize>> {
    static HANDLES: std::sync::OnceLock<Mutex<HashMap<i64, usize>>> = std::sync::OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_os = "macos")]
fn cancelled_downloads() -> &'static Mutex<HashSet<i64>> {
    static CANCELLED: std::sync::OnceLock<Mutex<HashSet<i64>>> = std::sync::OnceLock::new();
    CANCELLED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Bind the database row created inside Tauri's synchronous requested callback
/// to the native task currently passing through Wry's download delegate.
#[cfg(target_os = "macos")]
pub(crate) fn bind_current_native_download(id: i64) {
    CURRENT_DOWNLOAD_START.with(|current| {
        let pointer = current.get();
        if pointer == 0 {
            return;
        }
        unsafe {
            objc2::ffi::objc_retain(pointer as *mut AnyObject);
        }
        if let Ok(mut handles) = native_download_handles().lock() {
            if let Some(previous) = handles.insert(id, pointer) {
                unsafe { objc2::ffi::objc_release(previous as *mut AnyObject) };
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn bind_current_native_download(_id: i64) {}

/// Return the exact row whose native completion callback is currently running.
#[cfg(target_os = "macos")]
fn current_finished_native_download() -> Option<i64> {
    CURRENT_DOWNLOAD_FINISH.with(|current| match current.get() {
        0 => None,
        id => Some(id),
    })
}

#[cfg(not(target_os = "macos"))]
fn current_finished_native_download() -> Option<i64> {
    None
}

/// The failure callback produced by `WKDownload.cancel` is distinguished from
/// network failures while the native delegate is still inside the callback.
#[cfg(target_os = "macos")]
pub(crate) fn download_was_cancelled(id: i64) -> bool {
    cancelled_downloads()
        .lock()
        .ok()
        .is_some_and(|ids| ids.contains(&id))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn download_was_cancelled(_id: i64) -> bool {
    false
}

#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn download_policy_hook(
    this: *mut AnyObject,
    _command: Sel,
    download: *mut AnyObject,
    response: *mut AnyObject,
    filename: *mut AnyObject,
    completion: *mut AnyObject,
) {
    CURRENT_DOWNLOAD_START.with(|current| current.set(download as usize));
    let _: () = unsafe {
        msg_send![this, tds_download: download,
            decideDestinationUsingResponse: response,
            suggestedFilename: filename,
            completionHandler: completion]
    };
    CURRENT_DOWNLOAD_START.with(|current| current.set(0));
}

#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn download_finished_hook(
    this: *mut AnyObject,
    _command: Sel,
    download: *mut AnyObject,
) {
    native_download_completion(this, download, None, None, true);
}

#[cfg(target_os = "macos")]
unsafe extern "C-unwind" fn download_failed_hook(
    this: *mut AnyObject,
    _command: Sel,
    download: *mut AnyObject,
    error: *mut AnyObject,
    resume_data: *mut AnyObject,
) {
    native_download_completion(this, download, Some(error), Some(resume_data), false);
}

/// Wrap Wry's existing delegate instead of replacing it, preserving its path,
/// cookie and completion behavior while retaining a cancellable native handle.
#[cfg(target_os = "macos")]
fn native_download_completion(
    this: *mut AnyObject,
    download: *mut AnyObject,
    error: Option<*mut AnyObject>,
    resume_data: Option<*mut AnyObject>,
    success: bool,
) {
    let id = native_download_handles().lock().ok().and_then(|handles| {
        handles
            .iter()
            .find_map(|(id, pointer)| (*pointer == download as usize).then_some(*id))
    });
    CURRENT_DOWNLOAD_FINISH.with(|current| current.set(id.unwrap_or(0)));
    unsafe {
        if success {
            let _: () = msg_send![this, tds_downloadDidFinish: download];
        } else {
            let _: () = msg_send![this, tds_download: download,
                didFailWithError: error.unwrap_or(std::ptr::null_mut()),
                resumeData: resume_data.unwrap_or(std::ptr::null_mut())];
        }
    }
    CURRENT_DOWNLOAD_FINISH.with(|current| current.set(0));
    if let Some(id) = id {
        if let Ok(mut handles) = native_download_handles().lock() {
            if let Some(pointer) = handles.remove(&id) {
                unsafe { objc2::ffi::objc_release(pointer as *mut AnyObject) };
            }
        }
        if let Ok(mut cancelled) = cancelled_downloads().lock() {
            cancelled.remove(&id);
        }
    }
}

/// Install narrow wrappers around Wry's private download delegate after the
/// first child WebView registers that class. Runtime discovery avoids pinning
/// this adapter to Wry's versioned Objective-C class name.
#[cfg(target_os = "macos")]
fn install_native_download_hooks() -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.load(Ordering::Acquire) {
        return Ok(());
    }
    let count = unsafe { objc2::ffi::objc_getClassList(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return Err("无法读取浏览器下载适配器".into());
    }
    let mut classes = vec![std::ptr::null(); count as usize];
    let count = unsafe { objc2::ffi::objc_getClassList(classes.as_mut_ptr(), count) };
    let class = classes
        .into_iter()
        .take(count.max(0) as usize)
        .filter(|class| !class.is_null())
        .map(|class| unsafe { &*class })
        .find(|class: &&AnyClass| {
            class
                .name()
                .to_string_lossy()
                .contains("wry_download_delegate::WryDownloadDelegate")
        })
        .ok_or("浏览器下载适配器尚未初始化")?;
    unsafe {
        install_download_hook(
            class,
            objc2::sel!(download:decideDestinationUsingResponse:suggestedFilename:completionHandler:),
            objc2::sel!(tds_download:decideDestinationUsingResponse:suggestedFilename:completionHandler:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut AnyObject,
                    *mut AnyObject,
                    *mut AnyObject,
                    *mut AnyObject,
                ),
                objc2::runtime::Imp,
            >(download_policy_hook),
        )?;
        install_download_hook(
            class,
            objc2::sel!(downloadDidFinish:),
            objc2::sel!(tds_downloadDidFinish:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject),
                objc2::runtime::Imp,
            >(download_finished_hook),
        )?;
        install_download_hook(
            class,
            objc2::sel!(download:didFailWithError:resumeData:),
            objc2::sel!(tds_download:didFailWithError:resumeData:),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut AnyObject,
                    *mut AnyObject,
                    *mut AnyObject,
                ),
                objc2::runtime::Imp,
            >(download_failed_hook),
        )?;
    }
    INSTALLED.store(true, Ordering::Release);
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn install_download_hook(
    class: &AnyClass,
    original_selector: Sel,
    replacement_selector: Sel,
    replacement: objc2::runtime::Imp,
) -> Result<(), String> {
    let original = unsafe { objc2::ffi::class_getInstanceMethod(class, original_selector) };
    if original.is_null() {
        return Err(format!("缺少原生下载方法 {original_selector}"));
    }
    let encoding = unsafe { objc2::ffi::method_getTypeEncoding(original) };
    let added = unsafe {
        objc2::ffi::class_addMethod(
            (class as *const AnyClass).cast_mut(),
            replacement_selector,
            replacement,
            encoding,
        )
    };
    if !added.as_bool() {
        return Err(format!("无法安装原生下载方法 {replacement_selector}"));
    }
    let replacement = unsafe { objc2::ffi::class_getInstanceMethod(class, replacement_selector) };
    if replacement.is_null() {
        return Err(format!("无法读取原生下载方法 {replacement_selector}"));
    }
    unsafe {
        objc2::ffi::method_exchangeImplementations(original.cast_mut(), replacement.cast_mut());
    }
    Ok(())
}

/// Cancel the exact native task on AppKit; the resulting failure callback
/// updates SQLite to `cancelled` and removes the partial file.
#[cfg(target_os = "macos")]
pub(crate) fn cancel_download(app: &tauri::AppHandle, id: i64) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let result = (|| {
            let pointer = native_download_handles()
                .lock()
                .map_err(|error| error.to_string())?
                .get(&id)
                .copied()
                .ok_or_else(|| "下载任务已经结束".to_string())?;
            cancelled_downloads()
                .lock()
                .map_err(|error| error.to_string())?
                .insert(id);
            let download = unsafe { &*(pointer as *const WKDownload) };
            unsafe { download.cancel(None) };
            Ok(())
        })();
        let _ = sender.send(result);
    })
    .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "取消下载超时".to_string())?
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn cancel_download(_app: &tauri::AppHandle, _id: i64) -> Result<(), String> {
    Err("当前系统暂不支持取消内置浏览器下载".into())
}

#[cfg(target_os = "macos")]
struct EphemeralWebCryptoDelegateIvars {
    /// This key exists only for the lifetime of one temporary website data store.
    key: Zeroizing<[u8; 32]>,
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = EphemeralWebCryptoDelegateIvars]
    struct EphemeralWebCryptoDelegate;

    unsafe impl NSObjectProtocol for EphemeralWebCryptoDelegate {}

    impl EphemeralWebCryptoDelegate {
        /// WebKit calls this SPI before wrapping a serialized CryptoKey. Returning
        /// process-memory bytes prevents its fallback from querying macOS Keychain.
        #[unsafe(method(webCryptoMasterKey:))]
        fn web_crypto_master_key(
            &self,
            completion: &block2::Block<dyn Fn(*mut NSData)>,
        ) {
            let key = unsafe {
                NSData::dataWithBytes_length(
                    self.ivars().key.as_ptr().cast(),
                    self.ivars().key.len(),
                )
            };
            (*completion).call((Retained::as_ptr(&key).cast_mut(),));
        }
    }
);

#[cfg(target_os = "macos")]
impl EphemeralWebCryptoDelegate {
    /// Allocate a per-data-store key without touching any operating-system vault.
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        use aes_gcm::aead::rand_core::RngCore;

        let mut key = Zeroizing::new([0_u8; 32]);
        aes_gcm::aead::OsRng.fill_bytes(&mut *key);
        let delegate = mtm
            .alloc::<Self>()
            .set_ivars(EphemeralWebCryptoDelegateIvars { key });
        unsafe { msg_send![super(delegate), init] }
    }
}

/// A configuration created on the AppKit thread may be transferred without
/// access back to Tauri's dispatcher, which consumes it on that same thread.
/// `WKWebViewConfiguration` itself is intentionally not generally `Send`.
#[cfg(target_os = "macos")]
struct MainThreadWebviewConfiguration(Retained<WKWebViewConfiguration>);

#[cfg(target_os = "macos")]
// SAFETY: this wrapper is only used to carry an AppKit-created configuration
// into Tauri's synchronous main-thread builder call; it is never accessed on
// the command worker thread.
unsafe impl Send for MainThreadWebviewConfiguration {}

/// Configure macOS article views on the AppKit thread before navigation starts.
/// WebKit's private delegate hook is runtime-checked because it arrived in macOS 15.
#[cfg(target_os = "macos")]
fn article_webview_configuration_on_main() -> Result<Retained<WKWebViewConfiguration>, String> {
    use std::ffi::c_void;

    static DELEGATE_ASSOCIATION_KEY: u8 = 0;
    let mtm = MainThreadMarker::new().ok_or("浏览器配置必须在主线程创建")?;
    let configuration = unsafe { WKWebViewConfiguration::new(mtm) };
    let data_store = unsafe { WKWebsiteDataStore::nonPersistentDataStore(mtm) };
    unsafe { configuration.setWebsiteDataStore(&data_store) };

    if !data_store.respondsToSelector(objc2::sel!(set_delegate:)) {
        return Err("当前 macOS WebKit 无法隔离浏览器加密密钥".into());
    }
    let delegate = EphemeralWebCryptoDelegate::new(mtm);
    unsafe {
        let _: () = msg_send![&*data_store, set_delegate: &*delegate];
        // The WebKit SPI keeps only a weak delegate. Associate it strongly
        // with the temporary store so both have exactly the same lifetime.
        objc_setAssociatedObject(
            Retained::as_ptr(&data_store) as *mut AnyObject,
            (&DELEGATE_ASSOCIATION_KEY as *const u8).cast::<c_void>(),
            Retained::as_ptr(&delegate) as *mut AnyObject,
            OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
    }
    Ok(configuration)
}

/// Marshal native configuration creation to AppKit, then immediately return
/// ownership to Tauri's builder without accessing the object off that thread.
#[cfg(target_os = "macos")]
fn article_webview_configuration(
    app: &tauri::AppHandle,
) -> Result<Retained<WKWebViewConfiguration>, String> {
    if MainThreadMarker::new().is_some() {
        return article_webview_configuration_on_main();
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let result = article_webview_configuration_on_main().map(MainThreadWebviewConfiguration);
        let _ = sender.send(result);
    })
    .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "创建浏览器配置超时".to_string())?
        .map(|configuration| configuration.0)
}

/// Keep ordinary `target=_blank` links inside the single reading pane. The
/// native new-window handler below remains the security backstop for scripts
/// that call `window.open` directly.
const ARTICLE_INITIALIZATION_SCRIPT: &str = r#"
(() => {
  // WebKit only requests its application master key when a CryptoKey enters a
  // structured-clone path. Reader pages may use ordinary IndexedDB data, but
  // keys must stay in page memory so WebKit never reaches macOS Keychain.
  const CryptoKeyType = globalThis.CryptoKey;
  if (typeof CryptoKeyType !== 'function') return;

  const containsCryptoKey = (root) => {
    const pending = [root];
    const seen = new WeakSet();
    while (pending.length) {
      const value = pending.pop();
      if (value instanceof CryptoKeyType) return true;
      if (value === null || (typeof value !== 'object' && typeof value !== 'function')) continue;
      if (seen.has(value)) continue;
      seen.add(value);
      if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) continue;
      if (globalThis.Blob && value instanceof Blob) continue;
      if (value instanceof Date || value instanceof RegExp) continue;
      if (value instanceof Map) {
        for (const [key, item] of value) pending.push(key, item);
        continue;
      }
      if (value instanceof Set) {
        for (const item of value) pending.push(item);
        continue;
      }
      try {
        for (const descriptor of Object.values(Object.getOwnPropertyDescriptors(value))) {
          if ('value' in descriptor) pending.push(descriptor.value);
        }
      } catch (_) {}
    }
    return false;
  };

  const rejectCryptoKey = (value) => {
    if (containsCryptoKey(value)) {
      throw new DOMException(
        'CryptoKey persistence is disabled in the temporary reader',
        'DataCloneError',
      );
    }
  };
  const wrapFirstArgument = (prototype, method) => {
    if (!prototype) return;
    const descriptor = Object.getOwnPropertyDescriptor(prototype, method);
    if (!descriptor || typeof descriptor.value !== 'function') return;
    const nativeMethod = descriptor.value;
    try {
      Object.defineProperty(prototype, method, {
        ...descriptor,
        value(value, ...args) {
          rejectCryptoKey(value);
          return Reflect.apply(nativeMethod, this, [value, ...args]);
        },
      });
    } catch (_) {}
  };

  for (const [prototype, method] of [
    [globalThis.IDBObjectStore?.prototype, 'add'],
    [globalThis.IDBObjectStore?.prototype, 'put'],
    [globalThis.IDBCursor?.prototype, 'update'],
    [globalThis.Window?.prototype, 'postMessage'],
    [globalThis.Worker?.prototype, 'postMessage'],
    [globalThis.MessagePort?.prototype, 'postMessage'],
    [globalThis.BroadcastChannel?.prototype, 'postMessage'],
    [globalThis.ServiceWorker?.prototype, 'postMessage'],
  ]) wrapFirstArgument(prototype, method);

  const nativeStructuredClone = globalThis.structuredClone;
  if (typeof nativeStructuredClone === 'function') {
    try {
      Object.defineProperty(globalThis, 'structuredClone', {
        configurable: true,
        writable: true,
        value(value, options) {
          rejectCryptoKey(value);
          return nativeStructuredClone.call(this, value, options);
        },
      });
    } catch (_) {}
  }
})();
document.addEventListener('contextmenu', (event) => event.preventDefault(), true);
document.addEventListener('click', (event) => {
  const path = event.composedPath();
  const link = path.find((node) => node instanceof HTMLAnchorElement);
  if (!link || link.target.toLowerCase() !== '_blank') return;
  try {
    const target = new URL(link.href, document.baseURI);
    if (target.protocol !== 'http:' && target.protocol !== 'https:') return;
    event.preventDefault();
    window.location.assign(target.href);
  } catch (_) {}
}, true);
"#;

/// Apply tab-level website muting without exposing a command surface to the
/// untrusted page. The observer covers media elements added after page load.
fn apply_media_muting(view: &tauri::Webview, muted: bool) -> Result<(), String> {
    let script = format!(
        r#"(() => {{
  window.__topicDeskMuted = {muted};
  const apply = (root) => {{
    if (root instanceof HTMLMediaElement) root.muted = window.__topicDeskMuted;
    if (root.querySelectorAll) {{
      for (const media of root.querySelectorAll('audio, video')) media.muted = window.__topicDeskMuted;
    }}
  }};
  apply(document);
  if (!window.__topicDeskMuteObserver) {{
    window.__topicDeskMuteObserver = new MutationObserver((records) => {{
      for (const record of records) for (const node of record.addedNodes) {{
        if (node instanceof Element) apply(node);
      }}
    }});
    window.__topicDeskMuteObserver.observe(document.documentElement, {{ childList: true, subtree: true }});
  }}
}})()"#
    );
    view.eval(&script).map_err(|error| error.to_string())
}

/// CSS-pixel rectangle measured by the trusted UI; never supplied by remote pages.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub viewport_height: f64,
}

impl BrowserBounds {
    fn validate(&self) -> Result<(), String> {
        if [
            self.x,
            self.y,
            self.width,
            self.height,
            self.viewport_height,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width < 1.0
            || self.height < 1.0
            || self.viewport_height < 1.0
        {
            return Err("无效浏览区域".into());
        }
        Ok(())
    }
}

fn is_article_url(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && !matches!(url.host_str(), Some("tauri.localhost" | "ipc.localhost"))
        && !(matches!(url.host_str(), Some("127.0.0.1" | "localhost")) && url.port() == Some(1420))
}

/// Baidu's risk-control challenge opens a real auxiliary browsing context and
/// completes through `window.opener`. Replacing that popup with a navigation in
/// the article pane loses the opener and its ephemeral cookie session, so only
/// Baidu-owned identity/challenge hosts are allowed to create the native popup.
fn is_baidu_verification_url(url: &url::Url) -> bool {
    is_article_url(url)
        && matches!(
            url.host_str(),
            Some(
                "wappass.baidu.com"
                    | "passport.baidu.com"
                    | "verify.baidu.com"
                    | "seccenter.baidu.com"
            )
        )
}

/// Baidu's desktop search endpoint repeatedly challenges embedded WebKit even
/// after a successful CAPTCHA. Its official mobile endpoint serves the same
/// query and works in the constrained article view, so adapt only search-result
/// URLs while preserving the path, query and fragment verbatim.
fn adapt_article_url(mut url: url::Url) -> url::Url {
    if url.host_str() == Some("www.baidu.com") && url.path() == "/s" {
        // The replacement is a fixed valid domain; failure would indicate an
        // unexpected URL crate invariant, in which case the original is safer.
        let _ = url.set_host(Some("m.baidu.com"));
    }
    url
}

pub(crate) fn tab_label(tab_id: &str) -> Result<String, String> {
    if tab_id.is_empty()
        || tab_id.len() > 64
        || !tab_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return Err("无效浏览器标签页".into());
    }
    Ok(format!("article-{tab_id}"))
}

/// A readable main document ends the chrome spinner independently of slow embeds.
#[derive(Deserialize)]
struct DocumentReadiness {
    url: String,
    state: String,
    origin: f64,
}

impl DocumentReadiness {
    fn ready_for(&self, expected_url: &str, previous_origin: Option<f64>) -> bool {
        self.url == expected_url
            && matches!(self.state.as_str(), "interactive" | "complete")
            && self.origin.is_finite()
            && self.origin > 0.0
            && previous_origin != Some(self.origin)
    }
}

/// Poll only during one navigation. No script receives IPC privileges; probes
/// read the main document and stop on completion, replacement, closure or timeout.
fn watch_document_readiness(
    view: tauri::Webview,
    tab_id: String,
    url: String,
    generation: u64,
    previous_origin: Option<f64>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let loading = {
                let profile = view.app_handle().state::<BrowserProfile>();
                let Ok(session) = profile.inner.lock() else {
                    break;
                };
                let Some(tab) = session.tabs.get(&tab_id) else {
                    break;
                };
                if tab.load_generation != generation || tab.status.url != url {
                    break;
                }
                tab.status.loading
            };
            let (sender, receiver) = std::sync::mpsc::channel();
            if view
                .eval_with_callback(
                    "({url:location.href,state:document.readyState,origin:performance.timeOrigin})",
                    move |value| {
                        let _ = sender.send(value);
                    },
                )
                .is_err()
            {
                break;
            }
            if let Ok(raw) = receiver.recv_timeout(std::time::Duration::from_millis(500)) {
                if let Ok(readiness) = serde_json::from_str::<DocumentReadiness>(&raw) {
                    if readiness.ready_for(&url, previous_origin) {
                        browser_profile::page_ready(
                            view.app_handle(),
                            &tab_id,
                            &url,
                            generation,
                            readiness.origin,
                        );
                        break;
                    }
                }
            }
            if !loading {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });
}

fn hide_article_views(app: &tauri::AppHandle, except: Option<&str>) -> Result<(), String> {
    for (label, view) in app.webviews() {
        if label.starts_with("article-") && except != Some(label.as_str()) {
            view.hide().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Apply serialized UI requests off the UI thread (required by WebView2).
/// Capabilities target only the main webview, not every webview in its window.
pub fn browser_request(
    app: &tauri::AppHandle,
    action: &str,
    tab_id: Option<String>,
    url: Option<String>,
    bounds: Option<BrowserBounds>,
) -> Result<(), String> {
    let result = match action {
        "closeAll" => {
            {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.clear();
                session.active_tab = None;
            }
            for (label, view) in app.webviews() {
                if label.starts_with("article-") {
                    view.close().map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        }
        "hideAll" => {
            app.state::<BrowserProfile>()
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .active_tab = None;
            hide_article_views(app, None)
        }
        "close" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.remove(tab_id);
                if session.active_tab.as_deref() == Some(tab_id) {
                    session.active_tab = None;
                }
            }
            app.get_webview(&label).map_or(Ok(()), |view| {
                view.close().map_err(|error| error.to_string())
            })
        }
        "sync" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            let bounds = bounds.ok_or("缺少浏览区域")?;
            bounds.validate()?;
            let engine = app
                .state::<BrowserProfile>()
                .inner
                .lock()
                .map_err(|e| e.to_string())?
                .settings
                .search_engine
                .clone();
            let parsed = url
                .as_deref()
                .map(|input| browser_profile::resolve_address(input, &engine))
                .transpose()?
                .map(adapt_article_url);
            // The main webview can be inset by native titlebar decoration on macOS.
            // Translate DOM coordinates from that view into its parent window.
            let main = app.get_webview("main").ok_or("主界面不存在")?;
            let scale = main.window().scale_factor().map_err(|e| e.to_string())?;
            let origin = main
                .position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            // macOS extends the root NSView under the titlebar, but WKWebView's
            // DOM viewport excludes that safe area. Native window sizes include
            // it too, so compare the main webview height with the DOM height.
            #[cfg(target_os = "macos")]
            let titlebar = (main
                .size()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale)
                .height
                - bounds.viewport_height)
                .max(0.0);
            #[cfg(not(target_os = "macos"))]
            let titlebar = 0.0;
            let position =
                LogicalPosition::new(bounds.x + origin.x, bounds.y + origin.y + titlebar);
            let size = LogicalSize::new(bounds.width, bounds.height);
            hide_article_views(app, Some(&label))?;
            let target_loading = {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.active_tab = Some(tab_id.into());
                session
                    .tabs
                    .get(tab_id)
                    .is_some_and(|tab| tab.status.loading)
            };
            if let Some(view) = app.get_webview(&label) {
                view.set_bounds(tauri::Rect {
                    position: position.into(),
                    size: size.into(),
                })
                .map_err(|e| e.to_string())?;
                if !target_loading {
                    view.show().map_err(|e| e.to_string())?;
                }
                if let Some(url) = parsed {
                    view.navigate(url).map_err(|e| e.to_string())?;
                }
                Ok(())
            } else if let Some(url) = parsed {
                let popup_app = app.clone();
                let title_tab = tab_id.to_string();
                let page_tab = tab_id.to_string();
                let popup_tab = tab_id.to_string();
                let zoom = app
                    .state::<BrowserProfile>()
                    .inner
                    .lock()
                    .map_err(|e| e.to_string())?
                    .settings
                    .zoom;
                if !is_article_url(&url) {
                    return Err("不能在浏览区打开应用内部地址".into());
                }
                // WebKit reports no destination in the finished event on
                // macOS, so retain the native-assigned path by URL until the
                // matching completion arrives.
                let pending_downloads = Arc::new(Mutex::new(HashMap::<
                    String,
                    VecDeque<(Option<i64>, PathBuf)>,
                >::new()));
                let download_paths = pending_downloads.clone();
                let builder = WebviewBuilder::new(label, WebviewUrl::External(url))
                    // Ephemeral article views must never create WebCrypto or password material
                    // in the operating-system credential store.
                    .incognito(true)
                    // Run the storage isolation in every frame because third-party
                    // embeds can otherwise open their own persistent database.
                    .initialization_script_for_all_frames(ARTICLE_INITIALIZATION_SCRIPT)
                    .on_navigation(is_article_url)
                    .on_document_title_changed(move |view, title| {
                        browser_profile::title_changed(view.app_handle(), &title_tab, title)
                    })
                    .on_page_load(move |view, payload| {
                        // WebKit navigation callbacks can include child frames.
                        // Reading the WebView itself yields the canonical top URL.
                        let Ok(current) = view.url() else {
                            return;
                        };
                        // Child-frame events must not restart the top-level spinner.
                        if payload.url() != &current {
                            return;
                        }
                        match payload.event() {
                            PageLoadEvent::Started => {
                                if let Some((generation, previous_origin)) =
                                    browser_profile::navigated(
                                        view.app_handle(),
                                        &page_tab,
                                        current.as_str(),
                                    )
                                {
                                    watch_document_readiness(
                                        view.clone(),
                                        page_tab.clone(),
                                        current.to_string(),
                                        generation,
                                        previous_origin,
                                    );
                                }
                                // Let WebKit paint the response progressively instead of
                                // withholding usable content until every slow subresource
                                // finishes. The trusted React chrome keeps showing its
                                // loading indicator above the child view.
                                let active = view
                                    .app_handle()
                                    .state::<BrowserProfile>()
                                    .inner
                                    .lock()
                                    .ok()
                                    .is_some_and(|session| {
                                        session.active_tab.as_deref() == Some(&page_tab)
                                    });
                                if active {
                                    let _ = view.show();
                                }
                            }
                            PageLoadEvent::Finished => {
                                browser_profile::page_finished(
                                    view.app_handle(),
                                    &page_tab,
                                    current.as_str(),
                                );
                                let (muted, active) = view
                                    .app_handle()
                                    .state::<BrowserProfile>()
                                    .inner
                                    .lock()
                                    .ok()
                                    .map(|session| {
                                        (
                                            session
                                                .tabs
                                                .get(&page_tab)
                                                .is_some_and(|tab| tab.status.muted),
                                            session.active_tab.as_deref() == Some(&page_tab),
                                        )
                                    })
                                    .unwrap_or((false, false));
                                let _ = apply_media_muting(&view, muted);
                                if active {
                                    let _ = view.show();
                                }
                            }
                        }
                    })
                    .on_download(move |view, event| {
                        match event {
                            DownloadEvent::Requested { url, destination } => {
                                if !is_article_url(&url) {
                                    return false;
                                }
                                let Ok(directory) = view.app_handle().path().download_dir() else {
                                    return false;
                                };
                                let name = destination
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                let name: String = name
                                    .chars()
                                    .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
                                    .take(160)
                                    .collect();
                                *destination = directory.join(format!(
                                    "{}-{}",
                                    browser_profile::now(),
                                    if name.is_empty() { "download" } else { &name }
                                ));
                                let record_id = browser_profile::start_download(
                                    view.app_handle(),
                                    url.as_str(),
                                    destination,
                                )
                                .ok();
                                if let Ok(mut pending) = download_paths.lock() {
                                    pending
                                        .entry(url.to_string())
                                        .or_default()
                                        .push_back((record_id, destination.clone()));
                                }
                            }
                            DownloadEvent::Finished { url, path, success } => {
                                let finishing_id = current_finished_native_download();
                                let pending_download =
                                    download_paths.lock().ok().and_then(|mut pending| {
                                        let download =
                                            pending.get_mut(url.as_str()).and_then(|queue| {
                                                finishing_id
                                                    .and_then(|id| {
                                                        queue
                                                            .iter()
                                                            .position(|(record_id, _)| {
                                                                *record_id == Some(id)
                                                            })
                                                            .and_then(|index| queue.remove(index))
                                                    })
                                                    .or_else(|| queue.pop_front())
                                            });
                                        if pending.get(url.as_str()).is_some_and(VecDeque::is_empty)
                                        {
                                            pending.remove(url.as_str());
                                        }
                                        download
                                    });
                                if let Some((record_id, fallback)) = pending_download {
                                    let path = path.as_deref().unwrap_or(&fallback);
                                    if let Some(record_id) = record_id {
                                        browser_profile::finish_download(
                                            view.app_handle(),
                                            record_id,
                                            path,
                                            success,
                                        );
                                    } else {
                                        browser_profile::record_download(
                                            view.app_handle(),
                                            url.as_str(),
                                            path,
                                            if success { "complete" } else { "failed" },
                                        );
                                    }
                                } else if let Some(path) = path {
                                    browser_profile::record_download(
                                        view.app_handle(),
                                        url.as_str(),
                                        &path,
                                        if success { "complete" } else { "failed" },
                                    );
                                }
                            }
                            _ => (),
                        }
                        true
                    })
                    .on_new_window(move |url, _| {
                        // WebKit creates an allowed popup with the caller's
                        // configuration, preserving the non-persistent data
                        // store and opener relationship required by Baidu's
                        // verification callback. It remains isolated from all
                        // Tauri capabilities.
                        if is_baidu_verification_url(&url) {
                            return NewWindowResponse::Allow;
                        }
                        // Returning `Deny` cancels the popup navigation. Route the
                        // validated URL through the trusted main view afterward so
                        // the existing article view can navigate normally.
                        if is_article_url(&url) {
                            let _ = popup_app.emit_to(
                                EventTarget::webview("main"),
                                "browser-open-popup",
                                serde_json::json!({"tabId":popup_tab,"url":url.as_str()}),
                            );
                        }
                        NewWindowResponse::Deny
                    });
                #[cfg(target_os = "macos")]
                let builder =
                    builder.with_webview_configuration(article_webview_configuration(app)?);
                let view = app
                    .get_window("main")
                    .ok_or("主窗口不存在")?
                    .add_child(builder, position, size)
                    .map_err(|error| error.to_string())?;
                #[cfg(target_os = "macos")]
                install_native_download_hooks()?;
                view.set_zoom(zoom).map_err(|error| error.to_string())
            } else {
                Ok(())
            }
        }
        "back" | "forward" | "reload" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let label = tab_label(tab_id)?;
            let view = app.get_webview(&label).ok_or("浏览区域未打开")?;
            if action != "reload" {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                session.tabs.entry(tab_id.into()).or_default().traversal = true;
            }
            match action {
                "back" => view.eval("history.back()"),
                "forward" => view.eval("history.forward()"),
                _ => view.reload(),
            }
            .map_err(|error| error.to_string())
        }
        "mute" | "unmute" => {
            let tab_id = tab_id.as_deref().ok_or("缺少浏览器标签页")?;
            let muted = action == "mute";
            let status = {
                let profile = app.state::<BrowserProfile>();
                let mut session = profile.inner.lock().map_err(|e| e.to_string())?;
                let tab = session.tabs.get_mut(tab_id).ok_or("浏览区域未打开")?;
                tab.status.muted = muted;
                tab.status.clone()
            };
            if let Some(view) = app.get_webview(&tab_label(tab_id)?) {
                apply_media_muting(&view, muted)?;
            }
            browser_profile::publish(app, &status);
            Ok(())
        }
        _ => return Err("未知浏览操作".into()),
    };
    result.map_err(|error| error.to_string())
}

/// Capture only the web content through WKWebView, without desktop screen permission.
#[cfg(target_os = "macos")]
pub fn capture_screenshot(view: &tauri::Webview) -> Result<Vec<u8>, String> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKWebView;
    let (tx, rx) = std::sync::mpsc::channel();
    view.with_webview(move |platform| {
        let block = block2::RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let result = (|| {
                if !error.is_null() || image.is_null() {
                    return Err("网页截图失败".to_string());
                }
                // WebKit owns these callback pointers for the duration of this call.
                let image = unsafe { &*image };
                let tiff = image.TIFFRepresentation().ok_or("无法编码网页截图")?;
                let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                    .ok_or("无法读取网页截图")?;
                let png = unsafe {
                    bitmap.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &NSDictionary::new(),
                    )
                }
                .ok_or("无法生成 PNG")?;
                Ok(png.to_vec())
            })();
            let _ = tx.send(result);
        });
        // Tauri guarantees this callback executes on the UI thread with a live WKWebView.
        unsafe {
            (&*platform.inner().cast::<WKWebView>())
                .takeSnapshotWithConfiguration_completionHandler(None, &block);
        }
    })
    .map_err(|e| e.to_string())?;
    let bytes = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .map_err(|_| "网页截图超时")??;
    Ok(bytes)
}

/// Other desktop adapters keep the command explicit until their native capture is provided.
#[cfg(not(target_os = "macos"))]
pub fn capture_screenshot(_view: &tauri::Webview) -> Result<Vec<u8>, String> {
    Err("当前平台暂不支持原生网页截图".into())
}

/// Save a native viewport capture into a caller-controlled download destination.
pub fn save_screenshot(view: &tauri::Webview, path: &std::path::Path) -> Result<(), String> {
    std::fs::write(path, capture_screenshot(view)?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_document_finishes_before_slow_subresources_without_accepting_old_pages() {
        let mut document = DocumentReadiness {
            url: "https://example.com/article".into(),
            state: "interactive".into(),
            origin: 200.0,
        };
        assert!(document.ready_for("https://example.com/article", Some(100.0)));
        assert!(!document.ready_for("https://example.com/article", Some(200.0)));
        assert!(!document.ready_for("https://example.com/next", None));
        document.state = "loading".into();
        assert!(!document.ready_for("https://example.com/article", None));
        document.state = "complete".into();
        assert!(document.ready_for("https://example.com/article", None));
        document.origin = f64::NAN;
        assert!(!document.ready_for("https://example.com/article", None));
    }

    #[test]
    fn rejects_invalid_native_bounds() {
        let mut bounds = BrowserBounds {
            x: 220.0,
            y: 100.0,
            width: 600.0,
            height: 700.0,
            viewport_height: 800.0,
        };
        assert!(bounds.validate().is_ok());
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            bounds.width = width;
            assert!(bounds.validate().is_err());
        }
        bounds.width = 600.0;
        bounds.x = -10.0;
        assert!(bounds.validate().is_err());
    }

    #[test]
    fn accepts_web_articles_and_rejects_local_or_executable_urls() {
        for value in [
            "https://example.com/article?q=1#section",
            "http://example.com",
        ] {
            assert!(is_article_url(&url::Url::parse(value).unwrap()));
        }
        for value in [
            "not a url",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,test",
            "tauri://localhost",
            "about:blank",
        ] {
            if let Ok(url) = url::Url::parse(value) {
                assert!(!is_article_url(&url), "navigation must also reject {value}");
            }
        }
    }

    #[test]
    fn allows_only_baidu_identity_hosts_to_keep_popup_context() {
        for value in [
            "https://wappass.baidu.com/static/captcha/tuxing.html",
            "https://passport.baidu.com/v2/?login",
            "https://verify.baidu.com/challenge",
            "https://seccenter.baidu.com/security",
        ] {
            assert!(is_baidu_verification_url(&url::Url::parse(value).unwrap()));
        }
        for value in [
            "https://www.baidu.com/s?word=test",
            "https://wappass.baidu.com.evil.example/captcha",
            "http://passport.example.com/",
            "javascript:window.close()",
        ] {
            assert!(!is_baidu_verification_url(&url::Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn adapts_only_baidu_search_results_for_embedded_webkit() {
        let desktop =
            url::Url::parse("https://www.baidu.com/s?word=%E6%B5%8B%E8%AF%95&sa=fyb_news#results")
                .unwrap();
        assert_eq!(
            adapt_article_url(desktop).as_str(),
            "https://m.baidu.com/s?word=%E6%B5%8B%E8%AF%95&sa=fyb_news#results"
        );

        for value in [
            "https://www.baidu.com/",
            "https://top.baidu.com/board",
            "https://example.com/s?word=test",
        ] {
            let url = url::Url::parse(value).unwrap();
            assert_eq!(adapt_article_url(url.clone()), url);
        }
    }

    #[test]
    fn native_tab_labels_accept_only_safe_local_identifiers() {
        assert_eq!(tab_label("tab-42").unwrap(), "article-tab-42");
        for value in ["", "../main", "tab 1", "tab/1", &"a".repeat(65)] {
            assert!(tab_label(value).is_err(), "label must reject {value:?}");
        }
    }

    #[test]
    fn article_bootstrap_blocks_persistent_webcrypto_without_disabling_indexeddb() {
        for required_guard in [
            "IDBObjectStore?.prototype",
            "IDBCursor?.prototype",
            "CryptoKey persistence is disabled",
            "structuredClone",
            "contextmenu",
            "event.composedPath()",
            "window.location.assign",
        ] {
            assert!(
                ARTICLE_INITIALIZATION_SCRIPT.contains(required_guard),
                "reader bootstrap must retain {required_guard}"
            );
        }
        assert!(!ARTICLE_INITIALIZATION_SCRIPT.contains("IDBFactory?.prototype"));
        assert!(!ARTICLE_INITIALIZATION_SCRIPT.contains("deleteDatabase"));
    }
}
