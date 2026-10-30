//! Son du partage d'écran, comme l'appli Discord : seulement l'application
//! partagée (fenêtre), ou tout le PC sauf Plume (écran) pour que les voix de
//! l'appel ne repartent pas dans le stream. Capture audio par processus de
//! Windows (10 2004 et plus), en flottants 48 kHz stéréo, envoyée à la page
//! par paquets de 20 ms (`{plume:"audio"}`, lus par inject/screenshare.js).

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tauri::WebviewWindow;
use windows::core::{implement, Interface, Ref, HRESULT, HSTRING};
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// Trames par paquet envoyé à la page (20 ms).
const PACKET: usize = (RATE / 50) as usize;

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated(mpsc::Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

/// Client audio de capture pour `pid` (inclus, ou exclu avec ses enfants).
unsafe fn activate(pid: u32, exclude: bool) -> windows::core::Result<IAudioClient> {
    unsafe {
        let params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: if exclude { PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE } else { PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE },
                },
            },
        };
        // Jamais « libéré » : son Drop (PropVariantClear) rendrait au tas les
        // paramètres ci-dessus, qui sont sur la pile → corruption du tas.
        let variant = std::mem::ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    Anonymous: PROPVARIANT_0_0_0 { blob: BLOB { cbSize: std::mem::size_of_val(&params) as u32, pBlobData: &params as *const _ as *mut u8 } },
                    ..Default::default()
                }),
            },
        });
        let (tx, rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(tx).into();
        let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*variant as *const PROPVARIANT), &handler)?;
        rx.recv_timeout(Duration::from_secs(5)).map_err(|_| windows::core::Error::from_hresult(HRESULT(0x8000_000Au32 as i32)))?;
        let mut hr = HRESULT(0);
        let mut unknown = None;
        op.GetActivateResult(&mut hr, &mut unknown)?;
        hr.ok()?;
        unknown.ok_or_else(windows::core::Error::empty)?.cast()
    }
}

/// Capture en cours ; s'arrête quand on la lâche.
pub struct Loopback(Arc<AtomicBool>);

impl Drop for Loopback {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

fn post(window: &WebviewWindow, json: String) {
    let _ = window.with_webview(move |wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            let _ = core.PostWebMessageAsJson(&HSTRING::from(json));
        }
    });
}

/// Démarre la capture ; `Err` si Windows la refuse (version trop ancienne…).
pub fn start(window: WebviewWindow, pid: u32, exclude: bool) -> Result<Loopback, String> {
    let running = Arc::new(AtomicBool::new(true));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let flag = running.clone();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let result = (|| -> windows::core::Result<()> {
            let client = activate(pid, exclude)?;
            let format = WAVEFORMATEX {
                wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
                nChannels: CHANNELS,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * CHANNELS as u32 * 4,
                nBlockAlign: CHANNELS * 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                200_000,
                0,
                &format,
                None,
            )?;
            let event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            let _ = ready_tx.send(Ok(()));

            let mut pending: Vec<f32> = Vec::with_capacity(PACKET * 2 * CHANNELS as usize);
            while flag.load(Ordering::Relaxed) {
                if WaitForSingleObject(event, 100) != WAIT_OBJECT_0 {
                    continue;
                }
                while capture.GetNextPacketSize()? > 0 {
                    let mut data = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                    let samples = frames as usize * CHANNELS as usize;
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                        pending.extend(std::iter::repeat_n(0.0, samples));
                    } else {
                        pending.extend_from_slice(std::slice::from_raw_parts(data as *const f32, samples));
                    }
                    capture.ReleaseBuffer(frames)?;
                }
                let packet = PACKET * CHANNELS as usize;
                while pending.len() >= packet {
                    let bytes: Vec<u8> = pending.drain(..packet).flat_map(f32::to_le_bytes).collect();
                    post(&window, format!(r#"{{"plume":"audio","data":"{}"}}"#, crate::capture::base64(&bytes)));
                }
            }
            let _ = client.Stop();
            let _ = CloseHandle(event);
            Ok(())
        })();
        if let Err(e) = result {
            let _ = ready_tx.send(Err(e.to_string()));
        }
        CoUninitialize();
    });
    ready_rx.recv_timeout(Duration::from_secs(6)).map_err(|_| "capture audio sans réponse".to_string())??;
    Ok(Loopback(running))
}

