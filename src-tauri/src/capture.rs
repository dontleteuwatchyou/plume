//! Partage d'écran façon appli Discord : Plume liste les écrans et les fenêtres
//! (avec aperçus) pour son propre sélecteur (inject/screenshare.js), puis
//! capture la source choisie avec Windows.Graphics.Capture, comme OBS ou
//! l'appli Discord. Le sélecteur du navigateur n'apparaît jamais.
//!
//! Les images passent à la page par une mémoire partagée WebView2 (deux
//! emplacements, sans copie par l'IPC) ; un message par image dit lequel lire.
//! Réduction de taille sur la carte graphique (mipmaps), donc le processeur ne
//! copie que l'image finale.

use serde::Serialize;
use tauri::WebviewWindow;

/// Plus grande image transmise (et taille d'un emplacement de la mémoire partagée).
const MAX_W: u32 = 2560;
const MAX_H: u32 = 1440;
const SLOT: usize = (MAX_W * MAX_H * 4) as usize;

#[derive(Serialize)]
pub struct Source {
    /// `screen:<n>` ou `window:<hwnd>`.
    id: String,
    name: String,
    screen: bool,
    /// Aperçu en `data:image/bmp;base64,…`.
    thumb: String,
}

#[tauri::command]
pub async fn capture_sources() -> Vec<Source> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(win::sources).await.unwrap_or_default();
    #[cfg(not(windows))]
    Vec::new()
}

/// Démarre la capture ; la page reçoit d'abord la mémoire partagée, puis un
/// message `{plume:"frame"}` par image et `{plume:"ended"}` à la fin. Avec
/// `audio`, aussi le son (loopback.rs) ; renvoie vrai si le son a démarré.
#[tauri::command]
pub async fn capture_start(window: WebviewWindow, id: String, fps: u32, max_height: u32, audio: bool) -> Result<bool, String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || win::start(window, &id, fps.clamp(1, 60), max_height.clamp(144, MAX_H), audio))
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("Windows seulement".into())
}

#[tauri::command]
pub fn capture_stop(window: WebviewWindow) {
    #[cfg(windows)]
    win::stop(&window);
}

pub(crate) fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// BMP 32 bits (pixels BGRA ligne par ligne, du haut vers le bas).
fn bmp(w: u32, h: u32, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(54 + pixels.len());
    let size = 54 + pixels.len() as u32;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    out.extend_from_slice(pixels);
    out
}

/// Niveau de réduction (÷2 par niveau). On garde jusqu'à 1,5 × la hauteur
/// demandée : l'encodeur de Discord finit l'ajustement (1080p → 720p) mieux
/// qu'un ÷2 qui tomberait à 540p.
fn mip_level(w: u32, h: u32, max_h: u32) -> u32 {
    let mut level = 0;
    while (h >> level) > max_h * 3 / 2 || (w >> level) > MAX_W || (h >> level) > MAX_H {
        level += 1;
    }
    level
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tauri::WebviewWindow;
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use windows::core::{factory, Interface, BOOL, HSTRING};
    use windows::Foundation::TypedEventHandler;
    use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    use windows::Win32::UI::WindowsAndMessaging::*;

    use super::{base64, bmp, mip_level, Source, MAX_W, SLOT};

    const THUMB_W: i32 = 320;
    const THUMB_H: i32 = 180;

    // ---- Liste des sources -------------------------------------------------

    fn monitors() -> Vec<(HMONITOR, RECT)> {
        unsafe extern "system" fn cb(m: HMONITOR, _: HDC, r: *mut RECT, data: LPARAM) -> BOOL {
            let v = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, RECT)>) };
            v.push((m, unsafe { *r }));
            true.into()
        }
        let mut v: Vec<(HMONITOR, RECT)> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut v as *mut _ as isize));
        }
        // L'écran principal (toujours à l'origine) d'abord : c'est « Écran 1 ».
        v.sort_by_key(|(_, r)| (r.left, r.top) != (0, 0));
        v
    }

    /// Fenêtres qu'on peut partager : visibles, avec un titre, hors Plume.
    fn windows() -> Vec<(HWND, String)> {
        unsafe extern "system" fn cb(hwnd: HWND, data: LPARAM) -> BOOL {
            let v = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
            v.push(hwnd);
            true.into()
        }
        let mut all: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(cb), LPARAM(&mut all as *mut _ as isize));
        }
        let me = std::process::id();
        all.into_iter()
            .filter_map(|hwnd| unsafe {
                if !IsWindowVisible(hwnd).as_bool() || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                    return None;
                }
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                // Barres d'outils et calques (docks, barres des tâches tierces…).
                if ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
                    return None;
                }
                let mut cloaked = 0u32;
                let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
                if cloaked != 0 {
                    return None;
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == me {
                    return None;
                }
                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut buf);
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                // Le bureau lui-même (« Program Manager ») n'est pas une fenêtre à partager.
                (!title.trim().is_empty() && title != "Program Manager").then_some((hwnd, title))
            })
            .collect()
    }

    /// Aperçu 320×180 (proportions gardées) d'une zone d'un DC source.
    unsafe fn thumb_from(src: HDC, x: i32, y: i32, w: i32, h: i32) -> String {
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bmp_handle = CreateCompatibleBitmap(screen, THUMB_W, THUMB_H);
            let old = SelectObject(dc, bmp_handle.into());
            let scale = (THUMB_W as f32 / w.max(1) as f32).min(THUMB_H as f32 / h.max(1) as f32);
            let (tw, th) = ((w as f32 * scale) as i32, (h as f32 * scale) as i32);
            let (ox, oy) = ((THUMB_W - tw) / 2, (THUMB_H - th) / 2);
            SetStretchBltMode(dc, HALFTONE);
            let _ = StretchBlt(dc, ox, oy, tw, th, Some(src), x, y, w, h, SRCCOPY);
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: THUMB_W,
                    biHeight: -THUMB_H,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut px = vec![0u8; (THUMB_W * THUMB_H * 4) as usize];
            GetDIBits(dc, bmp_handle, 0, THUMB_H as u32, Some(px.as_mut_ptr() as _), &mut info, DIB_RGB_COLORS);
            // GDI laisse l'alpha à 0 : opaque partout.
            px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
            SelectObject(dc, old);
            let _ = DeleteObject(bmp_handle.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            format!("data:image/bmp;base64,{}", base64(&bmp(THUMB_W as u32, THUMB_H as u32, &px)))
        }
    }

    unsafe fn window_thumb(hwnd: HWND) -> String {
        unsafe {
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let full = CreateCompatibleBitmap(screen, w.max(1), h.max(1));
            let old = SelectObject(dc, full.into());
            // PW_RENDERFULLCONTENT : aussi les fenêtres dessinées par la carte graphique.
            let _ = PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(2));
            let thumb = thumb_from(dc, 0, 0, w, h);
            SelectObject(dc, old);
            let _ = DeleteObject(full.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            thumb
        }
    }

    pub fn sources() -> Vec<Source> {
        let mut out = Vec::new();
        unsafe {
            let screen = GetDC(None);
            for (i, (_, r)) in monitors().into_iter().enumerate() {
                out.push(Source {
                    id: format!("screen:{i}"),
                    name: format!("Écran {}", i + 1),
                    screen: true,
                    thumb: thumb_from(screen, r.left, r.top, r.right - r.left, r.bottom - r.top),
                });
            }
            ReleaseDC(None, screen);
            for (hwnd, title) in windows() {
                out.push(Source { id: format!("window:{}", hwnd.0 as isize), name: title, screen: false, thumb: window_thumb(hwnd) });
            }
        }
        out
    }

    // ---- Capture -----------------------------------------------------------

    struct Shared(ICoreWebView2SharedBuffer, *mut u8);
    // La mémoire partagée est écrite par le fil de capture ; l'objet COM n'est
    // fermé que sur le fil de l'interface (`stop`).
    unsafe impl Send for Shared {}

    struct Session {
        session: GraphicsCaptureSession,
        pool: Direct3D11CaptureFramePool,
        shared: Arc<Mutex<Option<Shared>>>,
        running: Arc<AtomicBool>,
        _sound: Option<crate::loopback::Loopback>,
    }

    static SESSION: Mutex<Option<Session>> = Mutex::new(None);

    /// Tout ce que le fil de capture garde d'une image à l'autre.
    struct Frames {
        device: ID3D11Device,
        ctx: ID3D11DeviceContext,
        d3d: IDirect3DDevice,
        size: SizeInt32,
        mip: Option<(ID3D11Texture2D, ID3D11ShaderResourceView, u32, u32)>,
        staging: Option<(ID3D11Texture2D, u32, u32)>,
        last: Instant,
        slot: usize,
    }
    unsafe impl Send for Frames {}

    fn post(window: &WebviewWindow, json: String) {
        let _ = window.with_webview(move |wv| unsafe {
            if let Ok(core) = wv.controller().CoreWebView2() {
                let _ = core.PostWebMessageAsJson(&HSTRING::from(json));
            }
        });
    }

    fn item_for(id: &str) -> Result<GraphicsCaptureItem, String> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(|e| e.to_string())?;
        unsafe {
            if let Some(n) = id.strip_prefix("screen:") {
                let n: usize = n.parse().map_err(|_| "écran inconnu")?;
                let (m, _) = *monitors().get(n).ok_or("écran introuvable")?;
                interop.CreateForMonitor(m).map_err(|e| e.to_string())
            } else if let Some(h) = id.strip_prefix("window:") {
                let h: isize = h.parse().map_err(|_| "fenêtre inconnue")?;
                interop.CreateForWindow(HWND(h as _)).map_err(|e| e.to_string())
            } else {
                Err("source inconnue".into())
            }
        }
    }

    pub fn start(window: WebviewWindow, id: &str, fps: u32, max_h: u32, audio: bool) -> Result<bool, String> {
        stop(&window);
        let item = item_for(id)?;

        let (device, ctx) = unsafe {
            let mut device = None;
            let mut ctx = None;
            D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, Default::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))
                .map_err(|e| e.to_string())?;
            (device.ok_or("pas de carte graphique")?, ctx.ok_or("pas de contexte graphique")?)
        };
        let d3d: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>().map_err(|e| e.to_string())?) }
            .and_then(|i| i.cast())
            .map_err(|e| e.to_string())?;

        // Mémoire partagée, créée et confiée à la page sur le fil de l'interface.
        let (tx, rx) = std::sync::mpsc::channel::<Result<Shared, String>>();
        window
            .with_webview(move |wv| unsafe {
                let r = (|| -> windows::core::Result<Shared> {
                    let env = wv.environment().cast::<ICoreWebView2Environment12>()?;
                    let buf = env.CreateSharedBuffer((SLOT * 2) as u64)?;
                    let mut ptr = std::ptr::null_mut();
                    buf.Buffer(&mut ptr)?;
                    let core = wv.controller().CoreWebView2()?.cast::<ICoreWebView2_17>()?;
                    core.PostSharedBufferToScript(&buf, COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, &HSTRING::from(r#"{"plume":"capture"}"#))?;
                    Ok(Shared(buf, ptr))
                })();
                let _ = tx.send(r.map_err(|e| e.to_string()));
            })
            .map_err(|e| e.to_string())?;
        let shared = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "page injoignable")??;
        let shared = Arc::new(Mutex::new(Some(shared)));

        let size = item.Size().map_err(|e| e.to_string())?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size).map_err(|e| e.to_string())?;
        let session = pool.CreateCaptureSession(&item).map_err(|e| e.to_string())?;
        let _ = session.SetIsCursorCaptureEnabled(true);
        // Sans le cadre jaune de Windows 11 quand c'est permis.
        let _ = session.SetIsBorderRequired(false);

        let running = Arc::new(AtomicBool::new(true));
        let frames = Mutex::new(Frames { device, ctx, d3d, size, mip: None, staging: None, last: Instant::now() - Duration::from_secs(1), slot: 0 });
        let interval = Duration::from_secs_f64(1.0 / fps as f64);
        let (w_frame, shared_frame, running_frame) = (window.clone(), shared.clone(), running.clone());
        pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(move |pool, _| {
            let Some(pool) = pool.as_ref() else { return Ok(()) };
            let frame = pool.TryGetNextFrame()?;
            if !running_frame.load(Ordering::Relaxed) {
                return Ok(());
            }
            let mut f = frames.lock().unwrap();
            let content = frame.ContentSize()?;
            if content.Width != f.size.Width || content.Height != f.size.Height {
                // Fenêtre redimensionnée : nouveau jeu de tampons à la bonne taille.
                f.size = content;
                pool.Recreate(&f.d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, content)?;
                f.mip = None;
                f.staging = None;
                return Ok(());
            }
            if f.last.elapsed() < interval {
                return Ok(());
            }
            f.last = Instant::now();
            let texture: ID3D11Texture2D = unsafe { frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()? };
            let (w, h) = (content.Width.max(1) as u32, content.Height.max(1) as u32);
            let level = mip_level(w, h, max_h);
            let (tw, th) = ((w >> level).max(1), (h >> level).max(1));
            if tw > MAX_W || (tw * th * 4) as usize > SLOT {
                return Ok(());
            }
            unsafe { copy_frame(&mut f, &texture, w, h, level, tw, th) }?;
            let slot = f.slot;
            f.slot ^= 1;
            let shared = shared_frame.lock().unwrap();
            let Some(Shared(_, base)) = shared.as_ref() else { return Ok(()) };
            let base = *base;
            unsafe { read_into(&f, tw, th, base.add(slot * SLOT)) }?;
            drop(shared);
            post(&w_frame, format!(r#"{{"plume":"frame","slot":{slot},"w":{tw},"h":{th}}}"#));
            Ok(())
        }))
        .map_err(|e| e.to_string())?;

        // Fenêtre partagée fermée : fin du partage, comme dans l'appli.
        let (w_closed, running_closed) = (window.clone(), running.clone());
        let _ = item.Closed(&TypedEventHandler::<GraphicsCaptureItem, windows::core::IInspectable>::new(move |_, _| {
            running_closed.store(false, Ordering::Relaxed);
            post(&w_closed, r#"{"plume":"ended"}"#.into());
            Ok(())
        }));

        session.StartCapture().map_err(|e| e.to_string())?;
        // Son : l'application de la fenêtre, ou tout sauf Plume pour un écran.
        let sound = audio
            .then(|| {
                let (pid, exclude) = match id.strip_prefix("window:").and_then(|h| h.parse::<isize>().ok()) {
                    Some(h) => {
                        let mut pid = 0u32;
                        unsafe { GetWindowThreadProcessId(HWND(h as _), Some(&mut pid)) };
                        (pid, false)
                    }
                    None => (std::process::id(), true),
                };
                crate::loopback::start(window.clone(), pid, exclude).map_err(|e| eprintln!("[plume] son du partage : {e}")).ok()
            })
            .flatten();
        let has_sound = sound.is_some();
        *SESSION.lock().unwrap() = Some(Session { session, pool, shared, running, _sound: sound });
        Ok(has_sound)
    }

    /// Copie l'image capturée dans la texture lisible, réduite de `level` crans.
    unsafe fn copy_frame(f: &mut Frames, texture: &ID3D11Texture2D, w: u32, h: u32, level: u32, tw: u32, th: u32) -> windows::core::Result<()> {
        unsafe {
            if f.staging.as_ref().is_none_or(|(_, sw, sh)| (*sw, *sh) != (tw, th)) {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: tw,
                    Height: th,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..Default::default()
                };
                let mut t = None;
                f.device.CreateTexture2D(&desc, None, Some(&mut t))?;
                f.staging = Some((t.unwrap(), tw, th));
            }
            let staging = f.staging.as_ref().unwrap().0.clone();
            let region = D3D11_BOX { left: 0, top: 0, front: 0, right: w, bottom: h, back: 1 };
            if level == 0 {
                f.ctx.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, Some(&region));
                return Ok(());
            }
            if f.mip.as_ref().is_none_or(|(_, _, mw, mh)| (*mw, *mh) != (w, h)) {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: w,
                    Height: h,
                    MipLevels: level + 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                    MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
                    ..Default::default()
                };
                let mut t = None;
                f.device.CreateTexture2D(&desc, None, Some(&mut t))?;
                let t = t.unwrap();
                let mut srv = None;
                f.device.CreateShaderResourceView(&t, None, Some(&mut srv))?;
                f.mip = Some((t, srv.unwrap(), w, h));
            }
            let (mip, srv, _, _) = f.mip.as_ref().unwrap();
            f.ctx.CopySubresourceRegion(mip, 0, 0, 0, 0, texture, 0, Some(&region));
            f.ctx.GenerateMips(srv);
            f.ctx.CopySubresourceRegion(&staging, 0, 0, 0, 0, mip, level, None);
            Ok(())
        }
    }

    /// Lit la texture lisible ligne par ligne vers la mémoire partagée.
    unsafe fn read_into(f: &Frames, tw: u32, th: u32, dest: *mut u8) -> windows::core::Result<()> {
        unsafe {
            let (staging, _, _) = f.staging.as_ref().unwrap();
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            f.ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            let row = (tw * 4) as usize;
            for y in 0..th as usize {
                std::ptr::copy_nonoverlapping((map.pData as *const u8).add(y * map.RowPitch as usize), dest.add(y * row), row);
            }
            f.ctx.Unmap(staging, 0);
            Ok(())
        }
    }

    pub fn stop(window: &WebviewWindow) {
        let Some(s) = SESSION.lock().unwrap().take() else { return };
        s.running.store(false, Ordering::Relaxed);
        let _ = s.session.Close();
        let _ = s.pool.Close();
        let shared = s.shared.lock().unwrap().take();
        if let Some(shared) = shared {
            let _ = window.with_webview(move |_| {
                // L'objet entier (Send), pas seulement son champ COM.
                let shared = shared;
                unsafe {
                    let _ = shared.0.Close();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_comme_le_standard() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }

    #[test]
    fn reduction_par_paliers() {
        assert_eq!(super::mip_level(1920, 1080, 1080), 0);
        assert_eq!(super::mip_level(3840, 2160, 1080), 1);
        assert_eq!(super::mip_level(1920, 1080, 720), 0);
        assert_eq!(super::mip_level(3840, 2160, 720), 1);
        assert_eq!(super::mip_level(5120, 1440, 1440), 1);
    }
}
