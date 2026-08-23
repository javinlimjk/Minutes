use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct SendStream(pub Option<cpal::Stream>);
unsafe impl Send for SendStream {}
unsafe impl Sync for SendStream {}

pub struct AudioEngine {
    pub is_recording: bool,
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: f32,
    pub stream: SendStream,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 16000.0,
            stream: SendStream(None),
        }
    }

    pub fn start_capture(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(target_os = "ios")]
        {
            if let Err(e) = init_ios_audio_session() {
                eprintln!("[AudioEngine] iOS AVAudioSession setup notice: {}", e);
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Err(e) = init_macos_audio_permission() {
                eprintln!("[AudioEngine] macOS AVCaptureDevice setup notice: {}", e);
            }
        }

        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(dev) => dev,
            None => {
                let err_msg = "No default audio input microphone device available on system";
                eprintln!("[AudioEngine] Error: {}", err_msg);
                return Err(err_msg.into());
            }
        };

        let supported_config = device.default_input_config()?;
        let sample_format = supported_config.sample_format();
        let config: cpal::StreamConfig = supported_config.into();
        let sample_rate = config.sample_rate.0 as f32;
        self.sample_rate = sample_rate;
        let channels = config.channels as usize;

        let dev_name = device.name().unwrap_or_else(|_| "Unknown Device".to_string());
        println!(
            "[AudioEngine] Initialized device '{}' | Format: {:?} | Sample Rate: {:.0}Hz | Channels: {}",
            dev_name, sample_format, sample_rate, channels
        );

        let buffer_clone = Arc::clone(&self.buffer);
        if let Ok(mut buf) = buffer_clone.lock() {
            buf.clear();
        }

        self.is_recording = true;

        let err_fn = move |err| {
            eprintln!("[AudioEngine] Audio input stream error: {:?}", err);
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let buf = Arc::clone(&buffer_clone);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                        for chunk in data.chunks(channels) {
                            let mono_sample = chunk.iter().sum::<f32>() / channels as f32;
                            b.push(mono_sample);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let buf = Arc::clone(&buffer_clone);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                        for chunk in data.chunks(channels) {
                            let mono_sample = chunk.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32;
                            b.push(mono_sample);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let buf = Arc::clone(&buffer_clone);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                        for chunk in data.chunks(channels) {
                            let mono_sample = chunk.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).sum::<f32>() / channels as f32;
                            b.push(mono_sample);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            fmt => {
                return Err(format!("Unsupported audio sample format: {:?}", fmt).into());
            }
        };

        stream.play()?;
        self.stream = SendStream(Some(stream));
        println!("[AudioEngine] Audio capture stream started successfully.");
        Ok(())
    }

    pub fn stop_capture(&mut self) -> Vec<f32> {
        self.is_recording = false;
        self.stream = SendStream(None);

        let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        let captured = buf.clone();
        buf.clear();

        let rms = if captured.is_empty() {
            0.0
        } else {
            (captured.iter().map(|&x| x * x).sum::<f32>() / captured.len() as f32).sqrt()
        };
        println!(
            "[AudioEngine] Audio stream stopped. Raw captured samples: {} at {:.0}Hz (Peak RMS: {:.5})",
            captured.len(),
            self.sample_rate,
            rms
        );

        let resampled = resample_to_16k(&captured, self.sample_rate);
        println!("[AudioEngine] Resampled output PCM (16000Hz mono): {} samples", resampled.len());
        resampled
    }
}

pub fn resample_to_16k(samples: &[f32], src_sample_rate: f32) -> Vec<f32> {
    let target_sample_rate = 16000.0;
    if samples.is_empty() {
        return Vec::new();
    }
    if (src_sample_rate - target_sample_rate).abs() < 1.0 {
        return samples.to_vec();
    }
    let ratio = src_sample_rate / target_sample_rate;
    let target_len = (samples.len() as f32 / ratio).floor() as usize;
    let mut resampled = Vec::with_capacity(target_len);

    let max_idx = samples.len() - 1;
    for i in 0..target_len {
        let src_index = i as f32 * ratio;
        let index_floor = (src_index.floor() as usize).min(max_idx);
        let index_ceil = (index_floor + 1).min(max_idx);
        let weight = src_index - index_floor as f32;

        let sample = samples[index_floor] * (1.0 - weight) + samples[index_ceil] * weight;
        resampled.push(sample);
    }
    resampled
}

#[cfg(target_os = "ios")]
fn init_ios_audio_session() -> Result<(), String> {
    unsafe {
        use std::ffi::c_void;
        #[link(name = "AVFoundation", kind = "framework")]
        extern "C" {
            fn objc_getClass(name: *const i8) -> *mut c_void;
            fn sel_registerName(name: *const i8) -> *mut c_void;
            fn objc_msgSend() -> *mut c_void;
        }

        let cls = objc_getClass(b"AVAudioSession\0".as_ptr() as *const i8);
        if cls.is_null() {
            return Err("Failed to get AVAudioSession class".to_string());
        }

        let shared_instance_sel = sel_registerName(b"sharedInstance\0".as_ptr() as *const i8);
        let msg_send_cls: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let session = msg_send_cls(cls, shared_instance_sel);

        if session.is_null() {
            return Err("Failed to get AVAudioSession sharedInstance".to_string());
        }

        let string_cls = objc_getClass(b"NSString\0".as_ptr() as *const i8);
        let string_utf8_sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const i8);
        let msg_send_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let category_str = msg_send_str(string_cls, string_utf8_sel, b"AVAudioSessionCategoryRecord\0".as_ptr() as *const i8);

        let set_category_sel = sel_registerName(b"setCategory:error:\0".as_ptr() as *const i8);
        let msg_send_category: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut *mut c_void) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        let mut err: *mut c_void = std::ptr::null_mut();
        let _ = msg_send_category(session, set_category_sel, category_str, &mut err);

        let set_active_sel = sel_registerName(b"setActive:error:\0".as_ptr() as *const i8);
        let msg_send_active: unsafe extern "C" fn(*mut c_void, *mut c_void, bool, *mut *mut c_void) -> bool =
            std::mem::transmute(objc_msgSend as *const ());
        let _ = msg_send_active(session, set_active_sel, true, &mut err);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn init_macos_audio_permission() -> Result<bool, String> {
    unsafe {
        use std::ffi::c_void;
        use std::sync::mpsc;
        use std::time::Duration;

        #[link(name = "AVFoundation", kind = "framework")]
        extern "C" {
            fn objc_getClass(name: *const i8) -> *mut c_void;
            fn sel_registerName(name: *const i8) -> *mut c_void;
            fn objc_msgSend() -> *mut c_void;
        }

        let cls = objc_getClass(c"AVCaptureDevice".as_ptr());
        if cls.is_null() {
            return Err("AVCaptureDevice class not found".to_string());
        }

        let string_cls = objc_getClass(c"NSString".as_ptr());
        let string_utf8_sel = sel_registerName(c"stringWithUTF8String:".as_ptr());
        let msg_send_str: unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let media_type = msg_send_str(string_cls, string_utf8_sel, c"soun".as_ptr());

        let auth_sel = sel_registerName(c"authorizationStatusForMediaType:".as_ptr());
        let msg_send_auth: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i64 =
            std::mem::transmute(objc_msgSend as *const ());
        let status = msg_send_auth(cls, auth_sel, media_type);

        println!("[AudioEngine] macOS AVCaptureDevice microphone authorization status: {}", status);

        // Status 3 means AVAuthorizationStatusAuthorized
        if status == 3 {
            return Ok(true);
        }

        let req_sel = sel_registerName(c"requestAccessForMediaType:completionHandler:".as_ptr());

        let (tx, rx) = mpsc::channel::<bool>();
        let tx_box = Box::into_raw(Box::new(tx));

        #[repr(C)]
        struct BlockDescriptor {
            reserved: usize,
            size: usize,
        }
        static DESCRIPTOR: BlockDescriptor = BlockDescriptor {
            reserved: 0,
            size: std::mem::size_of::<BlockLiteralWithCtx>(),
        };

        #[repr(C)]
        struct BlockLiteralWithCtx {
            isa: *const c_void,
            flags: i32,
            reserved: i32,
            invoke: unsafe extern "C" fn(*mut BlockLiteralWithCtx, bool),
            descriptor: *const BlockDescriptor,
            tx_ptr: *mut mpsc::Sender<bool>,
        }

        extern "C" {
            static _NSConcreteGlobalBlock: *const c_void;
        }

        unsafe extern "C" fn block_callback(block: *mut BlockLiteralWithCtx, granted: bool) {
            println!("[AudioEngine] macOS microphone permission prompt outcome: granted={}", granted);
            if !block.is_null() && !(*block).tx_ptr.is_null() {
                let tx = Box::from_raw((*block).tx_ptr);
                let _ = tx.send(granted);
            }
        }

        let block = Box::into_raw(Box::new(BlockLiteralWithCtx {
            isa: &_NSConcreteGlobalBlock as *const _ as *const c_void,
            flags: 1 << 28, // BLOCK_IS_GLOBAL
            reserved: 0,
            invoke: block_callback,
            descriptor: &DESCRIPTOR,
            tx_ptr: tx_box,
        }));

        let msg_send_req: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut BlockLiteralWithCtx) =
            std::mem::transmute(objc_msgSend as *const ());
        msg_send_req(cls, req_sel, media_type, block);

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(granted) => Ok(granted),
            Err(_) => {
                let new_status = msg_send_auth(cls, auth_sel, media_type);
                Ok(new_status == 3)
            }
        }
    }
}

pub fn save_pcm_to_wav(pcm_samples: &[f32], sample_rate: u32, path: &std::path::Path) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &sample in pcm_samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let s_i16 = (clamped * 32767.0) as i16;
        writer.write_sample(s_i16).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }

    Ok(())
}

pub fn read_wav_to_16k_pcm(path: &std::path::Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("Failed to open WAV: {:?}", e))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
        }
        hound::SampleFormat::Int => {
            match spec.bits_per_sample {
                16 => {
                    reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32768.0).collect()
                }
                24 => {
                    reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 8388608.0).collect()
                }
                32 => {
                    reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 2147483648.0).collect()
                }
                8 => {
                    reader.samples::<i8>().map(|s| s.unwrap_or(0) as f32 / 128.0).collect()
                }
                bits => return Err(format!("Unsupported integer bit depth: {}", bits)),
            }
        }
    };

    let mono = if spec.channels > 1 {
        samples.chunks(spec.channels as usize).map(|c| c.iter().sum::<f32>() / spec.channels as f32).collect()
    } else {
        samples
    };

    Ok(resample_to_16k(&mono, spec.sample_rate as f32))
}

