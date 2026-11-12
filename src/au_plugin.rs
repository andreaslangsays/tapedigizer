//! macOS AudioUnit v2 (`aufx` / `TdNr` / `TpDg`) Plug-In für Logic Pro & CoreAudio-Hosts.
//!
//! Teilt sich exakt dieselbe DSP-Engine (`SpectralDenoiser`, `DcSubsonicFilter`, `NoiseProfile`)
//! und denselben Preset-Ordner (`~/Library/Application Support/TapeDigizer/NoiseProfiles/`)
//! wie die Standalone-App `TapeDigizer`.

use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use serde::{Deserialize, Serialize};

use crate::audio::denoiser::{
    AuditionMode, DenoiserParams, NoisePresetManager, NoiseProfile,
    SpectralDenoiser, NUM_BINS, STFT_LATENCY_SAMPLES,
};

pub type OSStatus = i32;
pub type UInt32 = u32;
pub type SInt32 = i32;
pub type SInt16 = i16;
pub type Float32 = f32;
pub type Float64 = f64;
pub type Boolean = u8;
pub type AudioUnitPropertyID = UInt32;
pub type AudioUnitScope = UInt32;
pub type AudioUnitElement = UInt32;
pub type AudioUnitParameterID = UInt32;
pub type AudioUnitParameterValue = Float32;
pub type AudioUnitRenderActionFlags = UInt32;
pub type AudioChannelLayoutTag = UInt32;
pub type AudioComponentInstance = *mut c_void;
pub type AudioUnit = AudioComponentInstance;

pub type CFTypeRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFDataRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFIndex = isize;
pub type CFStringEncoding = UInt32;

pub const NO_ERR: OSStatus = 0;
pub const K_AUDIO_PARAM_ERR: OSStatus = -50;
pub const K_AUDIO_UNIMPLEMENTED_ERR: OSStatus = -4;

pub const K_AUDIO_UNIT_ERR_INVALID_PROPERTY: OSStatus = -10879;
pub const K_AUDIO_UNIT_ERR_INVALID_PARAMETER: OSStatus = -10878;
pub const K_AUDIO_UNIT_ERR_INVALID_ELEMENT: OSStatus = -10877;
pub const K_AUDIO_UNIT_ERR_NO_CONNECTION: OSStatus = -10876;
pub const K_AUDIO_UNIT_ERR_FAILED_INITIALIZATION: OSStatus = -10875;
pub const K_AUDIO_UNIT_ERR_TOO_MANY_FRAMES_TO_PROCESS: OSStatus = -10874;
pub const K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED: OSStatus = -10868;
pub const K_AUDIO_UNIT_ERR_UNINITIALIZED: OSStatus = -10867;
pub const K_AUDIO_UNIT_ERR_INVALID_SCOPE: OSStatus = -10866;
pub const K_AUDIO_UNIT_ERR_PROPERTY_NOT_WRITABLE: OSStatus = -10865;
pub const K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE: OSStatus = -10851;
pub const K_AUDIO_UNIT_ERR_PROPERTY_NOT_IN_USE: OSStatus = -10850;
pub const K_AUDIO_UNIT_ERR_INITIALIZED: OSStatus = -10849;
pub const K_AUDIO_UNIT_ERR_INVALID_PARAMETER_VALUE: OSStatus = -66743;

pub const K_AUDIO_UNIT_SCOPE_GLOBAL: AudioUnitScope = 0;
pub const K_AUDIO_UNIT_SCOPE_INPUT: AudioUnitScope = 1;
pub const K_AUDIO_UNIT_SCOPE_OUTPUT: AudioUnitScope = 2;

pub const K_AUDIO_UNIT_PROPERTY_CLASS_INFO: AudioUnitPropertyID = 0;
pub const K_AUDIO_UNIT_PROPERTY_MAKE_CONNECTION: AudioUnitPropertyID = 1;
pub const K_AUDIO_UNIT_PROPERTY_SAMPLE_RATE: AudioUnitPropertyID = 2;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_LIST: AudioUnitPropertyID = 3;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_INFO: AudioUnitPropertyID = 4;
pub const K_AUDIO_UNIT_PROPERTY_CPU_LOAD: AudioUnitPropertyID = 6;
pub const K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT: AudioUnitPropertyID = 8;
pub const K_AUDIO_UNIT_PROPERTY_ELEMENT_COUNT: AudioUnitPropertyID = 11;
pub const K_AUDIO_UNIT_PROPERTY_LATENCY: AudioUnitPropertyID = 12;
pub const K_AUDIO_UNIT_PROPERTY_SUPPORTED_NUM_CHANNELS: AudioUnitPropertyID = 13;
pub const K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE: AudioUnitPropertyID = 14;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_STRINGS: AudioUnitPropertyID = 16;
pub const K_AUDIO_UNIT_PROPERTY_AUDIO_CHANNEL_LAYOUT: AudioUnitPropertyID = 19;
pub const K_AUDIO_UNIT_PROPERTY_TAIL_TIME: AudioUnitPropertyID = 20;
pub const K_AUDIO_UNIT_PROPERTY_BYPASS_EFFECT: AudioUnitPropertyID = 21;
pub const K_AUDIO_UNIT_PROPERTY_LAST_RENDER_ERROR: AudioUnitPropertyID = 22;
pub const K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK: AudioUnitPropertyID = 23;
pub const K_AUDIO_UNIT_PROPERTY_FACTORY_PRESETS: AudioUnitPropertyID = 24;
pub const K_AUDIO_UNIT_PROPERTY_CONTEXT_NAME: AudioUnitPropertyID = 25;
pub const K_AUDIO_UNIT_PROPERTY_RENDER_QUALITY: AudioUnitPropertyID = 26;
pub const K_AUDIO_UNIT_PROPERTY_HOST_CALLBACKS: AudioUnitPropertyID = 27;
pub const K_AUDIO_UNIT_PROPERTY_IN_PLACE_PROCESSING: AudioUnitPropertyID = 29;
pub const K_AUDIO_UNIT_PROPERTY_ELEMENT_NAME: AudioUnitPropertyID = 30;
pub const K_AUDIO_UNIT_PROPERTY_SUPPORTED_CHANNEL_LAYOUT_TAGS: AudioUnitPropertyID = 32;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_STRING_FROM_VALUE: AudioUnitPropertyID = 33;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_ID_NAME: AudioUnitPropertyID = 34;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_CLUMP_NAME: AudioUnitPropertyID = 35;
pub const K_AUDIO_UNIT_PROPERTY_PRESENT_PRESET: AudioUnitPropertyID = 36;
pub const K_AUDIO_UNIT_PROPERTY_OFFLINE_RENDER: AudioUnitPropertyID = 37;
pub const K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_FROM_STRING: AudioUnitPropertyID = 38;
pub const K_AUDIO_UNIT_PROPERTY_PRESENTATION_LATENCY: AudioUnitPropertyID = 40;
pub const K_AUDIO_UNIT_PROPERTY_SHOULD_ALLOCATE_BUFFER: AudioUnitPropertyID = 51;

pub const K_AUDIO_UNIT_INITIALIZE_SELECT: SInt16 = 0x0001;
pub const K_AUDIO_UNIT_UNINITIALIZE_SELECT: SInt16 = 0x0002;
pub const K_AUDIO_UNIT_GET_PROPERTY_INFO_SELECT: SInt16 = 0x0003;
pub const K_AUDIO_UNIT_GET_PROPERTY_SELECT: SInt16 = 0x0004;
pub const K_AUDIO_UNIT_SET_PROPERTY_SELECT: SInt16 = 0x0005;
pub const K_AUDIO_UNIT_GET_PARAMETER_SELECT: SInt16 = 0x0006;
pub const K_AUDIO_UNIT_SET_PARAMETER_SELECT: SInt16 = 0x0007;
pub const K_AUDIO_UNIT_RESET_SELECT: SInt16 = 0x0009;
pub const K_AUDIO_UNIT_ADD_PROPERTY_LISTENER_SELECT: SInt16 = 0x000A;
pub const K_AUDIO_UNIT_REMOVE_PROPERTY_LISTENER_SELECT: SInt16 = 0x000B;
pub const K_AUDIO_UNIT_RENDER_SELECT: SInt16 = 0x000E;
pub const K_AUDIO_UNIT_ADD_RENDER_NOTIFY_SELECT: SInt16 = 0x000F;
pub const K_AUDIO_UNIT_REMOVE_RENDER_NOTIFY_SELECT: SInt16 = 0x0010;
pub const K_AUDIO_UNIT_SCHEDULE_PARAMETERS_SELECT: SInt16 = 0x0011;
pub const K_AUDIO_UNIT_REMOVE_PROPERTY_LISTENER_WITH_USER_DATA_SELECT: SInt16 = 0x0012;
pub const K_AUDIO_UNIT_PROCESS_SELECT: SInt16 = 0x0014;

pub const K_AUDIO_FORMAT_LINEAR_PCM: UInt32 = 0x6C70_636D; // 'lpcm'
pub const K_AUDIO_FORMAT_FLAG_IS_FLOAT: UInt32 = 1 << 0;
pub const K_AUDIO_FORMAT_FLAG_IS_BIG_ENDIAN: UInt32 = 1 << 1;
pub const K_AUDIO_FORMAT_FLAG_IS_PACKED: UInt32 = 1 << 3;
pub const K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED: UInt32 = 1 << 5;
pub const K_CANONICAL_AU_FLAGS: UInt32 = K_AUDIO_FORMAT_FLAG_IS_FLOAT
    | K_AUDIO_FORMAT_FLAG_IS_PACKED
    | K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED;

pub const K_AUDIO_UNIT_PARAMETER_UNIT_GENERIC: UInt32 = 0;
pub const K_AUDIO_UNIT_PARAMETER_UNIT_INDEXED: UInt32 = 1;
pub const K_AUDIO_UNIT_PARAMETER_UNIT_BOOLEAN: UInt32 = 2;
pub const K_AUDIO_UNIT_PARAMETER_UNIT_PERCENT: UInt32 = 3;
pub const K_AUDIO_UNIT_PARAMETER_UNIT_HERTZ: UInt32 = 8;
pub const K_AUDIO_UNIT_PARAMETER_UNIT_DECIBELS: UInt32 = 13;

pub const K_AUDIO_UNIT_PARAMETER_FLAG_CF_NAME_RELEASE: UInt32 = 1 << 4;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_VALUES_HAVE_STRINGS: UInt32 = 1 << 21;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_IS_HIGH_RESOLUTION: UInt32 = 1 << 23;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_CAN_RAMP: UInt32 = 1 << 25;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_HAS_CF_NAME_STRING: UInt32 = 1 << 27;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_IS_READABLE: UInt32 = 1 << 30;
pub const K_AUDIO_UNIT_PARAMETER_FLAG_IS_WRITABLE: UInt32 = 1 << 31;

pub const K_AUDIO_UNIT_RENDER_ACTION_PRE_RENDER: AudioUnitRenderActionFlags = 1 << 2;
pub const K_AUDIO_UNIT_RENDER_ACTION_POST_RENDER: AudioUnitRenderActionFlags = 1 << 3;

pub const AU_TYPE_EFFECT: i32 = 0x6175_6678; // 'aufx'
pub const AU_SUBTYPE_DENOISER: i32 = 0x5464_4E72; // 'TdNr'
pub const AU_MANUFACTURER_TAPEDIGIZER: i32 = 0x5470_4467; // 'TpDg'

pub const PARAM_PRESET: AudioUnitParameterID = 0;
pub const PARAM_REDUCTION_DB: AudioUnitParameterID = 1;
pub const PARAM_SENSITIVITY: AudioUnitParameterID = 2;
pub const PARAM_SMOOTHING: AudioUnitParameterID = 3;
pub const PARAM_DC_ENABLED: AudioUnitParameterID = 4;
pub const PARAM_DC_CUTOFF_HZ: AudioUnitParameterID = 5;
pub const PARAM_AUDITION_MODE: AudioUnitParameterID = 6;
pub const PARAM_LEARN_NOISE: AudioUnitParameterID = 7;
pub const NUM_PARAMETERS: usize = 8;

const BUILTIN_PRESET_COUNT: usize = 4;
const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;
const K_CF_NUMBER_SINT32_TYPE: CFIndex = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AudioComponentDescription {
    pub component_type: UInt32,
    pub component_sub_type: UInt32,
    pub component_manufacturer: UInt32,
    pub component_flags: UInt32,
    pub component_flags_mask: UInt32,
}

#[repr(C)]
pub struct AudioComponentPlugInInterface {
    pub open: unsafe extern "C" fn(self_ptr: *mut c_void, instance: AudioComponentInstance) -> OSStatus,
    pub close: unsafe extern "C" fn(self_ptr: *mut c_void) -> OSStatus,
    pub lookup: unsafe extern "C" fn(selector: SInt16) -> *const c_void,
    pub reserved: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AudioStreamBasicDescription {
    pub m_sample_rate: Float64,
    pub m_format_id: UInt32,
    pub m_format_flags: UInt32,
    pub m_bytes_per_packet: UInt32,
    pub m_frames_per_packet: UInt32,
    pub m_bytes_per_frame: UInt32,
    pub m_channels_per_frame: UInt32,
    pub m_bits_per_channel: UInt32,
    pub m_reserved: UInt32,
}

impl AudioStreamBasicDescription {
    pub fn canonical_stereo(sample_rate: Float64, channels: UInt32) -> Self {
        Self {
            m_sample_rate: sample_rate,
            m_format_id: K_AUDIO_FORMAT_LINEAR_PCM,
            m_format_flags: K_CANONICAL_AU_FLAGS,
            m_bytes_per_packet: 4,
            m_frames_per_packet: 1,
            m_bytes_per_frame: 4,
            m_channels_per_frame: channels,
            m_bits_per_channel: 32,
            m_reserved: 0,
        }
    }

    pub fn is_valid_canonical(&self) -> bool {
        if self.m_format_id != K_AUDIO_FORMAT_LINEAR_PCM {
            return false;
        }
        if self.m_sample_rate <= 0.0 || self.m_sample_rate > 384_000.0 || !self.m_sample_rate.is_finite() {
            return false;
        }
        if self.m_channels_per_frame != 1 && self.m_channels_per_frame != 2 {
            return false;
        }
        if self.m_bits_per_channel != 32 {
            return false;
        }
        if (self.m_format_flags & K_AUDIO_FORMAT_FLAG_IS_FLOAT) == 0 {
            return false;
        }
        if (self.m_format_flags & K_AUDIO_FORMAT_FLAG_IS_BIG_ENDIAN) != 0 {
            return false;
        }
        if (self.m_format_flags & K_AUDIO_FORMAT_FLAG_IS_PACKED) == 0 {
            return false;
        }
        if (self.m_format_flags & K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED) == 0 {
            return false;
        }
        if self.m_bytes_per_frame != 4 || self.m_bytes_per_packet != 4 || self.m_frames_per_packet != 1 {
            return false;
        }
        true
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AudioBuffer {
    pub m_number_channels: UInt32,
    pub m_data_byte_size: UInt32,
    pub m_data: *mut c_void,
}

#[repr(C)]
pub struct AudioBufferList {
    pub m_number_buffers: UInt32,
    pub m_buffers: [AudioBuffer; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AUChannelInfo {
    pub in_channels: SInt16,
    pub out_channels: SInt16,
}

pub type AURenderCallback = unsafe extern "C" fn(
    in_ref_con: *mut c_void,
    io_action_flags: *mut AudioUnitRenderActionFlags,
    in_time_stamp: *const c_void,
    in_bus_number: UInt32,
    in_number_frames: UInt32,
    io_data: *mut AudioBufferList,
) -> OSStatus;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AURenderCallbackStruct {
    pub input_proc: Option<AURenderCallback>,
    pub input_proc_ref_con: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioUnitConnection {
    pub source_audio_unit: AudioUnit,
    pub source_output_number: UInt32,
    pub dest_input_number: UInt32,
}

pub type AudioUnitPropertyListenerProc = unsafe extern "C" fn(
    in_ref_con: *mut c_void,
    in_unit: AudioUnit,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioUnitParameterInfo {
    pub name: [c_char; 52],
    pub unit_name: CFStringRef,
    pub clump_id: UInt32,
    pub cf_name_string: CFStringRef,
    pub unit: UInt32,
    pub min_value: AudioUnitParameterValue,
    pub max_value: AudioUnitParameterValue,
    pub default_value: AudioUnitParameterValue,
    pub flags: UInt32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AUPreset {
    pub preset_number: SInt32,
    pub preset_name: CFStringRef,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioUnitParameterStringFromValue {
    pub in_param_id: AudioUnitParameterID,
    pub in_value: *const AudioUnitParameterValue,
    pub out_string: CFStringRef,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioUnitParameterValueFromString {
    pub in_param_id: AudioUnitParameterID,
    pub in_string: CFStringRef,
    pub out_value: AudioUnitParameterValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImmediateEvent {
    pub buffer_offset: UInt32,
    pub value: AudioUnitParameterValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RampEvent {
    pub start_buffer_offset: SInt32,
    pub duration_in_frames: UInt32,
    pub start_value: AudioUnitParameterValue,
    pub end_value: AudioUnitParameterValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union EventValues {
    pub ramp: RampEvent,
    pub immediate: ImmediateEvent,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioUnitParameterEvent {
    pub scope: AudioUnitScope,
    pub element: AudioUnitElement,
    pub parameter: AudioUnitParameterID,
    pub event_type: UInt32,
    pub event_values: EventValues,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AudioChannelDescription {
    pub m_channel_label: UInt32,
    pub m_channel_flags: UInt32,
    pub m_coordinates: [Float32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AudioChannelLayout {
    pub m_channel_layout_tag: AudioChannelLayoutTag,
    pub m_channel_bitmap: UInt32,
    pub m_number_channel_descriptions: UInt32,
    pub m_channel_descriptions: [AudioChannelDescription; 1],
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFTypeArrayCallBacks: c_void;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;

    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: CFStringEncoding,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: CFStringEncoding,
    ) -> Boolean;
    fn CFArrayCreate(
        allocator: CFAllocatorRef,
        values: *const *const c_void,
        num_values: CFIndex,
        call_backs: *const c_void,
    ) -> CFArrayRef;
    fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: CFIndex,
        key_call_backs: *const c_void,
        value_call_backs: *const c_void,
    ) -> CFMutableDictionaryRef;
    fn CFDictionarySetValue(
        the_dict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );
    fn CFDictionaryGetValue(the_dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: CFIndex,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    fn CFNumberGetValue(
        number: CFNumberRef,
        the_type: CFIndex,
        value_ptr: *mut c_void,
    ) -> Boolean;
    fn CFDataCreate(
        allocator: CFAllocatorRef,
        bytes: *const u8,
        length: CFIndex,
    ) -> CFDataRef;
    fn CFDataGetBytePtr(the_data: CFDataRef) -> *const u8;
    fn CFDataGetLength(the_data: CFDataRef) -> CFIndex;
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioUnitRender(
        in_unit: AudioUnit,
        io_action_flags: *mut AudioUnitRenderActionFlags,
        in_time_stamp: *const c_void,
        in_output_bus_number: UInt32,
        in_number_frames: UInt32,
        io_data: *mut AudioBufferList,
    ) -> OSStatus;
}

fn make_cfstring(s: &str) -> CFStringRef {
    let clean = s.replace('\0', "");
    let cstr = CString::new(clean).unwrap_or_else(|_| CString::new("TapeDigizer").unwrap());
    unsafe { CFStringCreateWithCString(ptr::null(), cstr.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
}

fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    let mut buf = vec![0 as c_char; 512];
    let ok = unsafe {
        CFStringGetCString(
            cf,
            buf.as_mut_ptr(),
            buf.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if ok != 0 {
        let cstr = unsafe { CStr::from_ptr(buf.as_ptr()) };
        Some(cstr.to_string_lossy().into_owned())
    } else {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AuSavedState {
    parameters: [f32; NUM_PARAMETERS],
    preset_name: String,
    active_profile: Option<NoiseProfile>,
}

#[derive(Clone, Copy)]
pub struct PropertyListenerEntry {
    pub prop_id: AudioUnitPropertyID,
    pub proc: AudioUnitPropertyListenerProc,
    pub user_data: *mut c_void,
}

#[derive(Clone, Copy)]
pub struct RenderNotifyEntry {
    pub proc: AURenderCallback,
    pub user_data: *mut c_void,
}

/// Eingebaute Referenz-Tonbandprofile (immer verfügbar, zusätzlich zu den User-Presets
/// aus `~/Library/Application Support/TapeDigizer/NoiseProfiles/`).
fn builtin_noise_profile(index: usize, sample_rate: u32) -> NoiseProfile {
    let mut power_l = vec![0.0f32; NUM_BINS];
    let mut power_r = vec![0.0f32; NUM_BINS];

    let (name, media_source, base_floor_db, high_shelf_db, hum_50hz_db, dc_l) = match index {
        1 => (
            "Tonband 19 cm/s (NAB Studio)",
            "ReelToReel",
            -68.0f32,
            5.0f32,
            -62.0f32,
            0.0004f32,
        ),
        2 => (
            "Tonband 9,5 cm/s (Heimtonband)",
            "ReelToReel",
            -60.0f32,
            8.5f32,
            -56.0f32,
            0.0008f32,
        ),
        3 => (
            "Kassette Typ I (Ferric 120 µs)",
            "Cassette",
            -55.0f32,
            11.0f32,
            -58.0f32,
            0.0006f32,
        ),
        _ => (
            "Live-Learn / Standard Bandrauschen",
            "ReelToReel",
            -64.0f32,
            6.0f32,
            -60.0f32,
            0.0003f32,
        ),
    };

    let sr = sample_rate.max(8000) as f32;
    for k in 0..NUM_BINS {
        let freq = k as f32 * sr / 2048.0;
        let hiss_tilt = (freq / 2000.0).max(0.1).log10() * high_shelf_db;
        let hum = if (freq - 50.0).abs() < 35.0 || (freq - 100.0).abs() < 35.0 {
            10.0f32.powf(hum_50hz_db / 10.0)
        } else {
            0.0
        };
        let db = base_floor_db + hiss_tilt;
        let p = 10.0f32.powf(db / 10.0) + hum;
        power_l[k] = p;
        power_r[k] = p * 1.02;
    }

    NoiseProfile {
        name: name.to_string(),
        media_source: media_source.to_string(),
        sample_rate: sample_rate.max(8000),
        duration_secs: 1.5,
        dc_offset_l: dc_l,
        dc_offset_r: -dc_l * 0.8,
        rms_db_l: base_floor_db + 3.0,
        rms_db_r: base_floor_db + 3.1,
        noise_power_l: power_l,
        noise_power_r: power_r,
        wav_filename: None,
    }
}

#[repr(C)]
pub struct TapeDigizerAudioUnit {
    /// MUSS das erste Feld sein (`#[repr(C)]`), damit `*mut AudioComponentPlugInInterface`
    /// identisch mit `*mut TapeDigizerAudioUnit` ist.
    pub interface: AudioComponentPlugInInterface,
    pub component_instance: AudioComponentInstance,
    pub initialized: bool,
    pub input_format: AudioStreamBasicDescription,
    pub output_format: AudioStreamBasicDescription,
    pub max_frames_per_slice: UInt32,
    pub bypass: UInt32,
    pub in_place_processing: UInt32,
    pub should_allocate_input_buffer: UInt32,
    pub should_allocate_output_buffer: UInt32,
    pub render_quality: UInt32,
    pub offline_render: UInt32,
    pub last_render_error: OSStatus,
    pub presentation_latency_in: Float64,
    pub presentation_latency_out: Float64,
    pub context_name: CFStringRef,
    pub input_element_name: CFStringRef,
    pub output_element_name: CFStringRef,
    pub render_callback: AURenderCallbackStruct,
    pub connection: AudioUnitConnection,
    pub parameters: [AudioUnitParameterValue; NUM_PARAMETERS],
    pub present_preset_number: SInt32,
    pub present_preset_name: String,
    pub user_presets: Vec<NoiseProfile>,
    pub factory_presets_storage: Vec<AUPreset>,
    pub denoiser: SpectralDenoiser,
    pub scratch_in_l: Vec<f32>,
    pub scratch_in_r: Vec<f32>,
    pub scratch_out_l: Vec<f32>,
    pub scratch_out_r: Vec<f32>,
    pub learn_buf_l: Vec<f32>,
    pub learn_buf_r: Vec<f32>,
    pub learn_write_count: usize,
    pub was_learning: bool,
    pub property_listeners: Vec<PropertyListenerEntry>,
    pub render_notify_procs: Vec<RenderNotifyEntry>,
}

impl TapeDigizerAudioUnit {
    fn new() -> Box<Self> {
        let default_sr = 44100.0;
        let user_presets = NoisePresetManager::list_presets();
        let default_params = DenoiserParams::default();
        let mut denoiser = SpectralDenoiser::new(default_sr as u32);
        let initial_profile = if let Some(first_user) = user_presets.first() {
            first_user.clone()
        } else {
            builtin_noise_profile(1, default_sr as u32)
        };
        let initial_preset_idx = if !user_presets.is_empty() { 4.0 } else { 1.0 };
        let initial_preset_name = initial_profile.name.clone();
        denoiser.set_noise_profile(Some(initial_profile));

        let mut parameters = [0.0f32; NUM_PARAMETERS];
        parameters[PARAM_PRESET as usize] = initial_preset_idx;
        parameters[PARAM_REDUCTION_DB as usize] = default_params.reduction_db;
        parameters[PARAM_SENSITIVITY as usize] = default_params.sensitivity;
        parameters[PARAM_SMOOTHING as usize] = default_params.smoothing;
        parameters[PARAM_DC_ENABLED as usize] = if default_params.dc_filter_enabled { 1.0 } else { 0.0 };
        parameters[PARAM_DC_CUTOFF_HZ as usize] = default_params.dc_cutoff_hz;
        parameters[PARAM_AUDITION_MODE as usize] = 0.0;
        parameters[PARAM_LEARN_NOISE as usize] = 0.0;

        let max_frames = 4096usize;
        let learn_capacity = 48000 * 3; // bis zu 3 Sekunden Live-Learn im DAW-Track

        let mut unit = Box::new(Self {
            interface: AudioComponentPlugInInterface {
                open: au_open,
                close: au_close,
                lookup: au_lookup,
                reserved: ptr::null_mut(),
            },
            component_instance: ptr::null_mut(),
            initialized: false,
            input_format: AudioStreamBasicDescription::canonical_stereo(default_sr, 2),
            output_format: AudioStreamBasicDescription::canonical_stereo(default_sr, 2),
            max_frames_per_slice: max_frames as UInt32,
            bypass: 0,
            in_place_processing: 1,
            should_allocate_input_buffer: 1,
            should_allocate_output_buffer: 1,
            render_quality: 127,
            offline_render: 0,
            last_render_error: NO_ERR,
            presentation_latency_in: 0.0,
            presentation_latency_out: 0.0,
            context_name: ptr::null(),
            input_element_name: make_cfstring("Input"),
            output_element_name: make_cfstring("Output"),
            render_callback: AURenderCallbackStruct {
                input_proc: None,
                input_proc_ref_con: ptr::null_mut(),
            },
            connection: AudioUnitConnection {
                source_audio_unit: ptr::null_mut(),
                source_output_number: 0,
                dest_input_number: 0,
            },
            parameters,
            present_preset_number: initial_preset_idx as SInt32,
            present_preset_name: initial_preset_name,
            user_presets,
            factory_presets_storage: Vec::new(),
            denoiser,
            scratch_in_l: vec![0.0; max_frames],
            scratch_in_r: vec![0.0; max_frames],
            scratch_out_l: vec![0.0; max_frames],
            scratch_out_r: vec![0.0; max_frames],
            learn_buf_l: vec![0.0; learn_capacity],
            learn_buf_r: vec![0.0; learn_capacity],
            learn_write_count: 0,
            was_learning: false,
            property_listeners: Vec::new(),
            render_notify_procs: Vec::new(),
        });
        unit.rebuild_factory_presets();
        unit
    }

    fn total_preset_count(&self) -> usize {
        BUILTIN_PRESET_COUNT + self.user_presets.len()
    }

    fn preset_title(&self, index: usize) -> String {
        match index {
            0 => "Live-Learn / Manuell".to_string(),
            1 => "Tonband 19 cm/s (NAB Studio)".to_string(),
            2 => "Tonband 9,5 cm/s (Heimtonband)".to_string(),
            3 => "Kassette Typ I (Ferric 120 µs)".to_string(),
            idx => {
                let user_idx = idx.saturating_sub(BUILTIN_PRESET_COUNT);
                if let Some(entry) = self.user_presets.get(user_idx) {
                    format!("Sample: {}", entry.name)
                } else {
                    "Tonband 19 cm/s (NAB Studio)".to_string()
                }
            }
        }
    }

    fn rebuild_factory_presets(&mut self) {
        for preset in &self.factory_presets_storage {
            if !preset.preset_name.is_null() {
                unsafe { CFRelease(preset.preset_name) };
            }
        }
        self.factory_presets_storage.clear();
        let total = self.total_preset_count();
        for idx in 0..total {
            let title = self.preset_title(idx);
            let cf_title = make_cfstring(&title);
            self.factory_presets_storage.push(AUPreset {
                preset_number: idx as SInt32,
                preset_name: cf_title,
            });
        }
    }

    fn refresh_user_presets_from_disk(&mut self) {
        self.user_presets = NoisePresetManager::list_presets();
        self.rebuild_factory_presets();
    }

    fn apply_preset_by_index(&mut self, index: usize) {
        let sr = self.output_format.m_sample_rate.round().max(8000.0) as u32;
        if index >= BUILTIN_PRESET_COUNT {
            let user_idx = index - BUILTIN_PRESET_COUNT;
            if user_idx >= self.user_presets.len() {
                self.refresh_user_presets_from_disk();
            }
            if let Some(entry) = self.user_presets.get(user_idx).cloned() {
                self.present_preset_name = entry.name.clone();
                self.denoiser.set_noise_profile(Some(entry));
                return;
            }
        }
        if index == 0 {
            self.present_preset_name = "Live-Learn / Manuell".to_string();
            if self.denoiser.noise_profile().is_none() {
                self.denoiser.set_noise_profile(Some(builtin_noise_profile(0, sr)));
            }
        } else {
            let prof = builtin_noise_profile(index, sr);
            self.present_preset_name = prof.name.clone();
            self.denoiser.set_noise_profile(Some(prof));
        }
    }

    fn current_denoiser_params(&self) -> DenoiserParams {
        let mode_idx = self.parameters[PARAM_AUDITION_MODE as usize].round() as i32;
        let audition_mode = match mode_idx {
            1 => AuditionMode::Bypass,
            2 => AuditionMode::DeltaNoiseOnly,
            _ => AuditionMode::Wet,
        };
        DenoiserParams {
            enabled: self.bypass == 0,
            audition_mode,
            reduction_db: self.parameters[PARAM_REDUCTION_DB as usize].clamp(0.0, 36.0),
            sensitivity: self.parameters[PARAM_SENSITIVITY as usize].clamp(0.5, 3.5),
            smoothing: self.parameters[PARAM_SMOOTHING as usize].clamp(0.0, 0.95),
            dc_filter_enabled: self.parameters[PARAM_DC_ENABLED as usize] >= 0.5,
            dc_cutoff_hz: self.parameters[PARAM_DC_CUTOFF_HZ as usize].clamp(10.0, 60.0),
        }
    }

    fn notify_property_changed(
        &self,
        prop_id: AudioUnitPropertyID,
        scope: AudioUnitScope,
        elem: AudioUnitElement,
    ) {
        for listener in &self.property_listeners {
            if listener.prop_id == prop_id {
                unsafe {
                    (listener.proc)(
                        listener.user_data,
                        self.component_instance,
                        prop_id,
                        scope,
                        elem,
                    );
                }
            }
        }
    }

    fn ensure_scratch_capacity(&mut self, max_frames: usize) {
        let cap = max_frames.max(4096);
        if self.scratch_in_l.len() < cap {
            self.scratch_in_l.resize(cap, 0.0);
            self.scratch_in_r.resize(cap, 0.0);
            self.scratch_out_l.resize(cap, 0.0);
            self.scratch_out_r.resize(cap, 0.0);
        }
    }
}

impl Drop for TapeDigizerAudioUnit {
    fn drop(&mut self) {
        for preset in &self.factory_presets_storage {
            if !preset.preset_name.is_null() {
                unsafe { CFRelease(preset.preset_name) };
            }
        }
        if !self.context_name.is_null() {
            unsafe { CFRelease(self.context_name) };
        }
        if !self.input_element_name.is_null() {
            unsafe { CFRelease(self.input_element_name) };
        }
        if !self.output_element_name.is_null() {
            unsafe { CFRelease(self.output_element_name) };
        }
    }
}

/// Haupteinstiegspunkt (Factory Function) in `Info.plist` (`AudioComponents` -> `factoryFunction`).
#[no_mangle]
pub unsafe extern "C" fn TapeDigizerDenoiserFactory(
    _in_desc: *const AudioComponentDescription,
) -> *mut AudioComponentPlugInInterface {
    let unit = TapeDigizerAudioUnit::new();
    Box::into_raw(unit) as *mut AudioComponentPlugInInterface
}

unsafe extern "C" fn au_open(
    self_ptr: *mut c_void,
    instance: AudioComponentInstance,
) -> OSStatus {
    if self_ptr.is_null() {
        return K_AUDIO_PARAM_ERR;
    }
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.component_instance = instance;
    NO_ERR
}

unsafe extern "C" fn au_close(self_ptr: *mut c_void) -> OSStatus {
    if !self_ptr.is_null() {
        let _ = Box::from_raw(self_ptr as *mut TapeDigizerAudioUnit);
    }
    NO_ERR
}

unsafe extern "C" fn au_lookup(selector: SInt16) -> *const c_void {
    match selector {
        K_AUDIO_UNIT_INITIALIZE_SELECT => au_initialize as *const c_void,
        K_AUDIO_UNIT_UNINITIALIZE_SELECT => au_uninitialize as *const c_void,
        K_AUDIO_UNIT_GET_PROPERTY_INFO_SELECT => au_get_property_info as *const c_void,
        K_AUDIO_UNIT_GET_PROPERTY_SELECT => au_get_property as *const c_void,
        K_AUDIO_UNIT_SET_PROPERTY_SELECT => au_set_property as *const c_void,
        K_AUDIO_UNIT_GET_PARAMETER_SELECT => au_get_parameter as *const c_void,
        K_AUDIO_UNIT_SET_PARAMETER_SELECT => au_set_parameter as *const c_void,
        K_AUDIO_UNIT_RESET_SELECT => au_reset as *const c_void,
        K_AUDIO_UNIT_RENDER_SELECT => au_render as *const c_void,
        K_AUDIO_UNIT_PROCESS_SELECT => au_process as *const c_void,
        K_AUDIO_UNIT_SCHEDULE_PARAMETERS_SELECT => au_schedule_parameters as *const c_void,
        K_AUDIO_UNIT_ADD_PROPERTY_LISTENER_SELECT => au_add_property_listener as *const c_void,
        K_AUDIO_UNIT_REMOVE_PROPERTY_LISTENER_SELECT => au_remove_property_listener as *const c_void,
        K_AUDIO_UNIT_REMOVE_PROPERTY_LISTENER_WITH_USER_DATA_SELECT => {
            au_remove_property_listener_with_user_data as *const c_void
        }
        K_AUDIO_UNIT_ADD_RENDER_NOTIFY_SELECT => au_add_render_notify as *const c_void,
        K_AUDIO_UNIT_REMOVE_RENDER_NOTIFY_SELECT => au_remove_render_notify as *const c_void,
        _ => ptr::null(),
    }
}

unsafe extern "C" fn au_initialize(self_ptr: *mut c_void) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    if !unit.input_format.is_valid_canonical() || !unit.output_format.is_valid_canonical() {
        return K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED;
    }
    if unit.input_format.m_channels_per_frame != unit.output_format.m_channels_per_frame {
        return K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED;
    }
    if (unit.input_format.m_sample_rate - unit.output_format.m_sample_rate).abs() > 1.0 {
        return K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED;
    }

    let sr = unit.output_format.m_sample_rate.round().max(8000.0) as u32;
    unit.denoiser.set_sample_rate(sr);
    unit.denoiser.reset();
    unit.ensure_scratch_capacity(unit.max_frames_per_slice as usize);
    unit.learn_write_count = 0;
    unit.was_learning = false;
    unit.initialized = true;
    NO_ERR
}

unsafe extern "C" fn au_uninitialize(self_ptr: *mut c_void) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.denoiser.reset();
    unit.initialized = false;
    NO_ERR
}

unsafe extern "C" fn au_reset(
    self_ptr: *mut c_void,
    _in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.denoiser.reset();
    unit.learn_write_count = 0;
    unit.was_learning = false;
    NO_ERR
}

fn validate_io_element(scope: AudioUnitScope, elem: AudioUnitElement) -> Result<(), OSStatus> {
    match scope {
        K_AUDIO_UNIT_SCOPE_GLOBAL => {
            if elem != 0 {
                return Err(K_AUDIO_UNIT_ERR_INVALID_ELEMENT);
            }
            Ok(())
        }
        K_AUDIO_UNIT_SCOPE_INPUT | K_AUDIO_UNIT_SCOPE_OUTPUT => {
            if elem != 0 {
                return Err(K_AUDIO_UNIT_ERR_INVALID_ELEMENT);
            }
            Ok(())
        }
        _ => Err(K_AUDIO_UNIT_ERR_INVALID_SCOPE),
    }
}

unsafe extern "C" fn au_get_property_info(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    out_data_size: *mut UInt32,
    out_writable: *mut Boolean,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    let size: UInt32;
    let writable: Boolean;

    match in_id {
        K_AUDIO_UNIT_PROPERTY_ELEMENT_COUNT => {
            if in_scope > K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<UInt32>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<AudioStreamBasicDescription>() as UInt32;
            writable = if unit.initialized { 0 } else { 1 };
        }
        K_AUDIO_UNIT_PROPERTY_SAMPLE_RATE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<Float64>() as UInt32;
            writable = if unit.initialized { 0 } else { 1 };
        }
        K_AUDIO_UNIT_PROPERTY_SUPPORTED_NUM_CHANNELS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = (2 * std::mem::size_of::<AUChannelInfo>()) as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<UInt32>() as UInt32;
            writable = if unit.initialized { 0 } else { 1 };
        }
        K_AUDIO_UNIT_PROPERTY_LATENCY | K_AUDIO_UNIT_PROPERTY_TAIL_TIME | K_AUDIO_UNIT_PROPERTY_CPU_LOAD => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<Float64>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_BYPASS_EFFECT
        | K_AUDIO_UNIT_PROPERTY_IN_PLACE_PROCESSING
        | K_AUDIO_UNIT_PROPERTY_RENDER_QUALITY
        | K_AUDIO_UNIT_PROPERTY_OFFLINE_RENDER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<UInt32>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_SHOULD_ALLOCATE_BUFFER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<UInt32>() as UInt32;
            writable = if unit.initialized { 0 } else { 1 };
        }
        K_AUDIO_UNIT_PROPERTY_LAST_RENDER_ERROR => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<OSStatus>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<AURenderCallbackStruct>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_MAKE_CONNECTION => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<AudioUnitConnection>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_LIST => {
            if in_scope > K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = if in_scope == K_AUDIO_UNIT_SCOPE_GLOBAL {
                (NUM_PARAMETERS * std::mem::size_of::<AudioUnitParameterID>()) as UInt32
            } else {
                0
            };
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_INFO => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if (in_element as usize) >= NUM_PARAMETERS {
                return K_AUDIO_UNIT_ERR_INVALID_PARAMETER;
            }
            size = std::mem::size_of::<AudioUnitParameterInfo>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_STRINGS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != PARAM_PRESET && in_element != PARAM_AUDITION_MODE {
                return K_AUDIO_UNIT_ERR_INVALID_PROPERTY;
            }
            size = std::mem::size_of::<CFArrayRef>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_STRING_FROM_VALUE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            size = std::mem::size_of::<AudioUnitParameterStringFromValue>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_FROM_STRING => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            size = std::mem::size_of::<AudioUnitParameterValueFromString>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_FACTORY_PRESETS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<CFArrayRef>() as UInt32;
            writable = 0;
        }
        K_AUDIO_UNIT_PROPERTY_PRESENT_PRESET => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<AUPreset>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_CLASS_INFO => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<CFDictionaryRef>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_CONTEXT_NAME => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<CFStringRef>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_ELEMENT_NAME => {
            if let Err(err) = validate_io_element(in_scope, in_element) {
                return err;
            }
            size = std::mem::size_of::<CFStringRef>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_PRESENTATION_LATENCY => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = std::mem::size_of::<Float64>() as UInt32;
            writable = 1;
        }
        K_AUDIO_UNIT_PROPERTY_HOST_CALLBACKS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            size = 40; // sizeof(HostCallbackInfo) on 64-bit
            writable = 1;
        }
        _ => return K_AUDIO_UNIT_ERR_INVALID_PROPERTY,
    }

    if !out_data_size.is_null() {
        *out_data_size = size;
    }
    if !out_writable.is_null() {
        *out_writable = writable;
    }
    NO_ERR
}

fn fill_parameter_info(unit: &TapeDigizerAudioUnit, param_id: AudioUnitParameterID) -> Result<AudioUnitParameterInfo, OSStatus> {
    let mut info = AudioUnitParameterInfo {
        name: [0; 52],
        unit_name: ptr::null(),
        clump_id: 1,
        cf_name_string: ptr::null(),
        unit: K_AUDIO_UNIT_PARAMETER_UNIT_GENERIC,
        min_value: 0.0,
        max_value: 1.0,
        default_value: 0.0,
        flags: K_AUDIO_UNIT_PARAMETER_FLAG_IS_READABLE
            | K_AUDIO_UNIT_PARAMETER_FLAG_IS_WRITABLE
            | K_AUDIO_UNIT_PARAMETER_FLAG_HAS_CF_NAME_STRING
            | K_AUDIO_UNIT_PARAMETER_FLAG_CF_NAME_RELEASE,
    };

    let (title, unit_kind, min_v, max_v, def_v, extra_flags) = match param_id {
        PARAM_PRESET => {
            let max_idx: f32 = (unit.total_preset_count().saturating_sub(1)) as f32;
            let def_idx: f32 = if !unit.user_presets.is_empty() { 4.0 } else { 1.0 };
            (
                "Noise Sample Preset",
                K_AUDIO_UNIT_PARAMETER_UNIT_INDEXED,
                0.0,
                max_idx.max(0.0),
                def_idx.min(max_idx),
                K_AUDIO_UNIT_PARAMETER_FLAG_VALUES_HAVE_STRINGS,
            )
        }
        PARAM_REDUCTION_DB => (
            "Noise Reduction",
            K_AUDIO_UNIT_PARAMETER_UNIT_DECIBELS,
            0.0,
            36.0,
            15.0,
            K_AUDIO_UNIT_PARAMETER_FLAG_CAN_RAMP | K_AUDIO_UNIT_PARAMETER_FLAG_IS_HIGH_RESOLUTION,
        ),
        PARAM_SENSITIVITY => (
            "Sensitivity (Threshold)",
            K_AUDIO_UNIT_PARAMETER_UNIT_GENERIC,
            0.5,
            3.5,
            1.5,
            K_AUDIO_UNIT_PARAMETER_FLAG_CAN_RAMP | K_AUDIO_UNIT_PARAMETER_FLAG_IS_HIGH_RESOLUTION,
        ),
        PARAM_SMOOTHING => (
            "Spectral Smoothing",
            K_AUDIO_UNIT_PARAMETER_UNIT_PERCENT,
            0.0,
            95.0,
            65.0,
            K_AUDIO_UNIT_PARAMETER_FLAG_CAN_RAMP | K_AUDIO_UNIT_PARAMETER_FLAG_IS_HIGH_RESOLUTION,
        ),
        PARAM_DC_ENABLED => (
            "DC & Subsonic Filter",
            K_AUDIO_UNIT_PARAMETER_UNIT_BOOLEAN,
            0.0,
            1.0,
            1.0,
            0,
        ),
        PARAM_DC_CUTOFF_HZ => (
            "DC Cutoff Frequency",
            K_AUDIO_UNIT_PARAMETER_UNIT_HERTZ,
            10.0,
            60.0,
            20.0,
            K_AUDIO_UNIT_PARAMETER_FLAG_CAN_RAMP | K_AUDIO_UNIT_PARAMETER_FLAG_IS_HIGH_RESOLUTION,
        ),
        PARAM_AUDITION_MODE => (
            "Audition Mode",
            K_AUDIO_UNIT_PARAMETER_UNIT_INDEXED,
            0.0,
            2.0,
            0.0,
            K_AUDIO_UNIT_PARAMETER_FLAG_VALUES_HAVE_STRINGS,
        ),
        PARAM_LEARN_NOISE => (
            "Learn Noise from Track",
            K_AUDIO_UNIT_PARAMETER_UNIT_BOOLEAN,
            0.0,
            1.0,
            0.0,
            0,
        ),
        _ => return Err(K_AUDIO_UNIT_ERR_INVALID_PARAMETER),
    };

    let bytes = title.as_bytes();
    let copy_len = bytes.len().min(51);
    for (i, &b) in bytes[..copy_len].iter().enumerate() {
        info.name[i] = b as c_char;
    }
    info.cf_name_string = make_cfstring(title);
    info.unit = unit_kind;
    info.min_value = min_v;
    info.max_value = max_v;
    info.default_value = def_v;
    info.flags |= extra_flags;
    Ok(info)
}

unsafe extern "C" fn au_get_property(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    out_data: *mut c_void,
    io_data_size: *mut UInt32,
) -> OSStatus {
    if io_data_size.is_null() {
        return K_AUDIO_PARAM_ERR;
    }
    if out_data.is_null() {
        return au_get_property_info(self_ptr, in_id, in_scope, in_element, io_data_size, ptr::null_mut());
    }
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);

    match in_id {
        K_AUDIO_UNIT_PROPERTY_ELEMENT_COUNT => {
            if in_scope > K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = 1;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let fmt = if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.input_format
            } else {
                unit.output_format
            };
            *(out_data as *mut AudioStreamBasicDescription) = fmt;
            *io_data_size = std::mem::size_of::<AudioStreamBasicDescription>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SAMPLE_RATE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let sr = if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.input_format.m_sample_rate
            } else {
                unit.output_format.m_sample_rate
            };
            *(out_data as *mut Float64) = sr;
            *io_data_size = std::mem::size_of::<Float64>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SUPPORTED_NUM_CHANNELS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let info = [
                AUChannelInfo {
                    in_channels: 1,
                    out_channels: 1,
                },
                AUChannelInfo {
                    in_channels: 2,
                    out_channels: 2,
                },
            ];
            let dst = out_data as *mut AUChannelInfo;
            *dst = info[0];
            *dst.add(1) = info[1];
            *io_data_size = (2 * std::mem::size_of::<AUChannelInfo>()) as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = unit.max_frames_per_slice;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_LATENCY | K_AUDIO_UNIT_PROPERTY_TAIL_TIME => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let sr = unit.output_format.m_sample_rate.max(8000.0);
            *(out_data as *mut Float64) = (STFT_LATENCY_SAMPLES as Float64) / sr;
            *io_data_size = std::mem::size_of::<Float64>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_CPU_LOAD => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut Float64) = 0.0;
            *io_data_size = std::mem::size_of::<Float64>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_BYPASS_EFFECT => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = unit.bypass;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_IN_PLACE_PROCESSING => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = unit.in_place_processing;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_RENDER_QUALITY => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = unit.render_quality;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_OFFLINE_RENDER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = unit.offline_render;
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SHOULD_ALLOCATE_BUFFER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut UInt32) = if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.should_allocate_input_buffer
            } else {
                unit.should_allocate_output_buffer
            };
            *io_data_size = std::mem::size_of::<UInt32>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_LAST_RENDER_ERROR => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            *(out_data as *mut OSStatus) = unit.last_render_error;
            unit.last_render_error = NO_ERR;
            *io_data_size = std::mem::size_of::<OSStatus>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_LIST => {
            if in_scope > K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_scope == K_AUDIO_UNIT_SCOPE_GLOBAL {
                let dst = out_data as *mut AudioUnitParameterID;
                for i in 0..NUM_PARAMETERS {
                    *dst.add(i) = i as AudioUnitParameterID;
                }
                *io_data_size = (NUM_PARAMETERS * std::mem::size_of::<AudioUnitParameterID>()) as UInt32;
            } else {
                *io_data_size = 0;
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_INFO => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            match fill_parameter_info(unit, in_element) {
                Ok(info) => {
                    *(out_data as *mut AudioUnitParameterInfo) = info;
                    *io_data_size = std::mem::size_of::<AudioUnitParameterInfo>() as UInt32;
                    NO_ERR
                }
                Err(e) => e,
            }
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_STRINGS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            let strings: Vec<CFStringRef> = if in_element == PARAM_PRESET {
                let total = unit.total_preset_count();
                (0..total).map(|idx| make_cfstring(&unit.preset_title(idx))).collect()
            } else if in_element == PARAM_AUDITION_MODE {
                vec![
                    make_cfstring("Cleaned (Wet)"),
                    make_cfstring("Bypass (Original)"),
                    make_cfstring("Noise Only (Delta)"),
                ]
            } else {
                return K_AUDIO_UNIT_ERR_INVALID_PROPERTY;
            };
            let array = CFArrayCreate(
                ptr::null(),
                strings.as_ptr() as *const *const c_void,
                strings.len() as CFIndex,
                &kCFTypeArrayCallBacks,
            );
            for s in strings {
                CFRelease(s);
            }
            *(out_data as *mut CFArrayRef) = array;
            *io_data_size = std::mem::size_of::<CFArrayRef>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_STRING_FROM_VALUE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if (*io_data_size as usize) < std::mem::size_of::<AudioUnitParameterStringFromValue>() {
                return K_AUDIO_PARAM_ERR;
            }
            let req = &mut *(out_data as *mut AudioUnitParameterStringFromValue);
            let param_id = req.in_param_id;
            if (param_id as usize) >= NUM_PARAMETERS {
                return K_AUDIO_UNIT_ERR_INVALID_PARAMETER;
            }
            let val = if req.in_value.is_null() {
                unit.parameters[param_id as usize]
            } else {
                *req.in_value
            };
            let idx = val.round().max(0.0) as usize;
            let text = match param_id {
                PARAM_PRESET => unit.preset_title(idx.min(unit.total_preset_count().saturating_sub(1))),
                PARAM_AUDITION_MODE => match idx {
                    1 => "Bypass (Original)".to_string(),
                    2 => "Noise Only (Delta)".to_string(),
                    _ => "Cleaned (Wet)".to_string(),
                },
                _ => return K_AUDIO_UNIT_ERR_INVALID_PROPERTY,
            };
            req.out_string = make_cfstring(&text);
            *io_data_size = std::mem::size_of::<AudioUnitParameterStringFromValue>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_FROM_STRING => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if (*io_data_size as usize) < std::mem::size_of::<AudioUnitParameterValueFromString>() {
                return K_AUDIO_PARAM_ERR;
            }
            let req = &mut *(out_data as *mut AudioUnitParameterValueFromString);
            let param_id = req.in_param_id;
            let Some(query) = cfstring_to_string(req.in_string) else {
                return K_AUDIO_PARAM_ERR;
            };
            match param_id {
                PARAM_PRESET => {
                    let total = unit.total_preset_count();
                    let mut found = 0.0f32;
                    for i in 0..total {
                        if unit.preset_title(i) == query {
                            found = i as f32;
                            break;
                        }
                    }
                    req.out_value = found;
                }
                PARAM_AUDITION_MODE => {
                    req.out_value = match query.as_str() {
                        "Bypass (Original)" => 1.0,
                        "Noise Only (Delta)" => 2.0,
                        _ => 0.0,
                    };
                }
                _ => return K_AUDIO_UNIT_ERR_INVALID_PROPERTY,
            }
            *io_data_size = std::mem::size_of::<AudioUnitParameterValueFromString>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_FACTORY_PRESETS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            unit.rebuild_factory_presets();
            let ptrs: Vec<*const c_void> = unit
                .factory_presets_storage
                .iter()
                .map(|p| p as *const AUPreset as *const c_void)
                .collect();
            let array = CFArrayCreate(
                ptr::null(),
                ptrs.as_ptr(),
                ptrs.len() as CFIndex,
                ptr::null(),
            );
            *(out_data as *mut CFArrayRef) = array;
            *io_data_size = std::mem::size_of::<CFArrayRef>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PRESENT_PRESET => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let cf_name = make_cfstring(&unit.present_preset_name);
            *(out_data as *mut AUPreset) = AUPreset {
                preset_number: unit.present_preset_number,
                preset_name: cf_name,
            };
            *io_data_size = std::mem::size_of::<AUPreset>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_CLASS_INFO => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            let dict = CFDictionaryCreateMutable(
                ptr::null(),
                8,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            let set_int = |key_s: &str, val: i32| {
                let k = make_cfstring(key_s);
                let num = CFNumberCreate(
                    ptr::null(),
                    K_CF_NUMBER_SINT32_TYPE,
                    &val as *const i32 as *const c_void,
                );
                CFDictionarySetValue(dict, k, num);
                CFRelease(k);
                CFRelease(num);
            };
            set_int("version", 0);
            set_int("type", AU_TYPE_EFFECT);
            set_int("subtype", AU_SUBTYPE_DENOISER);
            set_int("manufacturer", AU_MANUFACTURER_TAPEDIGIZER);

            let k_name = make_cfstring("name");
            let v_name = make_cfstring(&unit.present_preset_name);
            CFDictionarySetValue(dict, k_name, v_name);
            CFRelease(k_name);
            CFRelease(v_name);

            let saved = AuSavedState {
                parameters: unit.parameters,
                preset_name: unit.present_preset_name.clone(),
                active_profile: unit.denoiser.noise_profile().cloned(),
            };
            if let Ok(json_bytes) = serde_json::to_vec(&saved) {
                let k_data = make_cfstring("data");
                let v_data = CFDataCreate(ptr::null(), json_bytes.as_ptr(), json_bytes.len() as CFIndex);
                CFDictionarySetValue(dict, k_data, v_data);
                CFRelease(k_data);
                CFRelease(v_data);
            }

            *(out_data as *mut CFDictionaryRef) = dict as CFDictionaryRef;
            *io_data_size = std::mem::size_of::<CFDictionaryRef>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_CONTEXT_NAME => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if unit.context_name.is_null() {
                return K_AUDIO_UNIT_ERR_PROPERTY_NOT_IN_USE;
            }
            CFRetain(unit.context_name);
            *(out_data as *mut CFStringRef) = unit.context_name;
            *io_data_size = std::mem::size_of::<CFStringRef>() as UInt32;
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_ELEMENT_NAME => {
            if let Err(err) = validate_io_element(in_scope, in_element) {
                return err;
            }
            let s = match in_scope {
                K_AUDIO_UNIT_SCOPE_INPUT => unit.input_element_name,
                K_AUDIO_UNIT_SCOPE_OUTPUT => unit.output_element_name,
                _ => ptr::null(),
            };
            if s.is_null() {
                return K_AUDIO_UNIT_ERR_PROPERTY_NOT_IN_USE;
            }
            CFRetain(s);
            *(out_data as *mut CFStringRef) = s;
            *io_data_size = std::mem::size_of::<CFStringRef>() as UInt32;
            NO_ERR
        }
        _ => K_AUDIO_UNIT_ERR_INVALID_PROPERTY,
    }
}

unsafe extern "C" fn au_set_property(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    in_data: *const c_void,
    in_data_size: UInt32,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);

    match in_id {
        K_AUDIO_UNIT_PROPERTY_STREAM_FORMAT => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if unit.initialized {
                return K_AUDIO_UNIT_ERR_INITIALIZED;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<AudioStreamBasicDescription>() {
                return K_AUDIO_PARAM_ERR;
            }
            let fmt = *(in_data as *const AudioStreamBasicDescription);
            if !fmt.is_valid_canonical() {
                return K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED;
            }
            if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.input_format = fmt;
            } else {
                unit.output_format = fmt;
            }
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SAMPLE_RATE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if unit.initialized {
                return K_AUDIO_UNIT_ERR_INITIALIZED;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<Float64>() {
                return K_AUDIO_PARAM_ERR;
            }
            let sr = *(in_data as *const Float64);
            if sr <= 0.0 || sr > 384_000.0 || !sr.is_finite() {
                return K_AUDIO_UNIT_ERR_FORMAT_NOT_SUPPORTED;
            }
            if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.input_format.m_sample_rate = sr;
            } else {
                unit.output_format.m_sample_rate = sr;
            }
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_MAXIMUM_FRAMES_PER_SLICE => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if unit.initialized {
                return K_AUDIO_UNIT_ERR_INITIALIZED;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            let max_frames = *(in_data as *const UInt32);
            unit.max_frames_per_slice = max_frames.max(1);
            unit.ensure_scratch_capacity(unit.max_frames_per_slice as usize);
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_BYPASS_EFFECT => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            unit.bypass = if *(in_data as *const UInt32) != 0 { 1 } else { 0 };
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_IN_PLACE_PROCESSING => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            unit.in_place_processing = if *(in_data as *const UInt32) != 0 { 1 } else { 0 };
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_RENDER_QUALITY => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            unit.render_quality = *(in_data as *const UInt32);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_OFFLINE_RENDER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            unit.offline_render = if *(in_data as *const UInt32) != 0 { 1 } else { 0 };
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SHOULD_ALLOCATE_BUFFER => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if unit.initialized {
                return K_AUDIO_UNIT_ERR_INITIALIZED;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<UInt32>() {
                return K_AUDIO_PARAM_ERR;
            }
            let val = if *(in_data as *const UInt32) != 0 { 1 } else { 0 };
            if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.should_allocate_input_buffer = val;
            } else {
                unit.should_allocate_output_buffer = val;
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_SET_RENDER_CALLBACK => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<AURenderCallbackStruct>() {
                unit.render_callback = AURenderCallbackStruct {
                    input_proc: None,
                    input_proc_ref_con: ptr::null_mut(),
                };
            } else {
                unit.render_callback = *(in_data as *const AURenderCallbackStruct);
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_MAKE_CONNECTION => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<AudioUnitConnection>() {
                unit.connection.source_audio_unit = ptr::null_mut();
            } else {
                unit.connection = *(in_data as *const AudioUnitConnection);
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PRESENT_PRESET => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<AUPreset>() {
                return K_AUDIO_PARAM_ERR;
            }
            let preset = *(in_data as *const AUPreset);
            if preset.preset_number >= 0 {
                let idx = preset.preset_number as usize;
                if idx >= unit.total_preset_count() {
                    return K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE;
                }
                unit.present_preset_number = preset.preset_number;
                unit.parameters[PARAM_PRESET as usize] = idx as f32;
                unit.apply_preset_by_index(idx);
            } else {
                if preset.preset_name.is_null() {
                    return K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE;
                }
                unit.present_preset_number = preset.preset_number;
                unit.present_preset_name = cfstring_to_string(preset.preset_name)
                    .unwrap_or_else(|| "User Preset".to_string());
            }
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_CLASS_INFO => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<CFDictionaryRef>() {
                return K_AUDIO_PARAM_ERR;
            }
            let dict = *(in_data as *const CFDictionaryRef);
            if dict.is_null() {
                return K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE;
            }
            let get_int = |key_s: &str| -> Option<i32> {
                let k = make_cfstring(key_s);
                let val_ptr = CFDictionaryGetValue(dict, k);
                CFRelease(k);
                if val_ptr.is_null() {
                    return None;
                }
                let mut out: i32 = 0;
                if CFNumberGetValue(
                    val_ptr as CFNumberRef,
                    K_CF_NUMBER_SINT32_TYPE,
                    &mut out as *mut i32 as *mut c_void,
                ) != 0
                {
                    Some(out)
                } else {
                    None
                }
            };
            if get_int("version") != Some(0) {
                return K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE;
            }
            if get_int("type") != Some(AU_TYPE_EFFECT)
                || get_int("subtype") != Some(AU_SUBTYPE_DENOISER)
                || get_int("manufacturer") != Some(AU_MANUFACTURER_TAPEDIGIZER)
            {
                return K_AUDIO_UNIT_ERR_INVALID_PROPERTY_VALUE;
            }
            let k_data = make_cfstring("data");
            let data_ptr = CFDictionaryGetValue(dict, k_data);
            CFRelease(k_data);
            if !data_ptr.is_null() {
                let cf_data = data_ptr as CFDataRef;
                let len = CFDataGetLength(cf_data);
                let bytes_ptr = CFDataGetBytePtr(cf_data);
                if len > 0 && !bytes_ptr.is_null() {
                    let slice = std::slice::from_raw_parts(bytes_ptr, len as usize);
                    if let Ok(saved) = serde_json::from_slice::<AuSavedState>(slice) {
                        unit.parameters = saved.parameters;
                        unit.present_preset_name = saved.preset_name;
                        if let Some(prof) = saved.active_profile {
                            unit.denoiser.set_noise_profile(Some(prof));
                        }
                    }
                }
            }
            unit.notify_property_changed(in_id, in_scope, in_element);
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_CONTEXT_NAME => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if !unit.context_name.is_null() {
                CFRelease(unit.context_name);
                unit.context_name = ptr::null();
            }
            if !in_data.is_null() && (in_data_size as usize) >= std::mem::size_of::<CFStringRef>() {
                let s = *(in_data as *const CFStringRef);
                if !s.is_null() {
                    CFRetain(s);
                    unit.context_name = s;
                }
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_ELEMENT_NAME => {
            if let Err(err) = validate_io_element(in_scope, in_element) {
                return err;
            }
            let slot = match in_scope {
                K_AUDIO_UNIT_SCOPE_INPUT => &mut unit.input_element_name,
                K_AUDIO_UNIT_SCOPE_OUTPUT => &mut unit.output_element_name,
                _ => return NO_ERR,
            };
            if !(*slot).is_null() {
                CFRelease(*slot);
                *slot = ptr::null();
            }
            if !in_data.is_null() && (in_data_size as usize) >= std::mem::size_of::<CFStringRef>() {
                let s = *(in_data as *const CFStringRef);
                if !s.is_null() {
                    CFRetain(s);
                    *slot = s;
                }
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_PRESENTATION_LATENCY => {
            if in_scope != K_AUDIO_UNIT_SCOPE_INPUT && in_scope != K_AUDIO_UNIT_SCOPE_OUTPUT {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            if in_data.is_null() || (in_data_size as usize) < std::mem::size_of::<Float64>() {
                return K_AUDIO_PARAM_ERR;
            }
            let lat = *(in_data as *const Float64);
            if in_scope == K_AUDIO_UNIT_SCOPE_INPUT {
                unit.presentation_latency_in = lat;
            } else {
                unit.presentation_latency_out = lat;
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_HOST_CALLBACKS => {
            if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
                return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
            }
            if in_element != 0 {
                return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
            }
            NO_ERR
        }
        K_AUDIO_UNIT_PROPERTY_ELEMENT_COUNT
        | K_AUDIO_UNIT_PROPERTY_SUPPORTED_NUM_CHANNELS
        | K_AUDIO_UNIT_PROPERTY_LATENCY
        | K_AUDIO_UNIT_PROPERTY_TAIL_TIME
        | K_AUDIO_UNIT_PROPERTY_CPU_LOAD
        | K_AUDIO_UNIT_PROPERTY_LAST_RENDER_ERROR
        | K_AUDIO_UNIT_PROPERTY_PARAMETER_LIST
        | K_AUDIO_UNIT_PROPERTY_PARAMETER_INFO
        | K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_STRINGS
        | K_AUDIO_UNIT_PROPERTY_PARAMETER_STRING_FROM_VALUE
        | K_AUDIO_UNIT_PROPERTY_PARAMETER_VALUE_FROM_STRING
        | K_AUDIO_UNIT_PROPERTY_FACTORY_PRESETS => K_AUDIO_UNIT_ERR_PROPERTY_NOT_WRITABLE,
        _ => K_AUDIO_UNIT_ERR_INVALID_PROPERTY,
    }
}

unsafe extern "C" fn au_get_parameter(
    self_ptr: *mut c_void,
    in_id: AudioUnitParameterID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    out_value: *mut AudioUnitParameterValue,
) -> OSStatus {
    if out_value.is_null() {
        return K_AUDIO_PARAM_ERR;
    }
    if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
        return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
    }
    if in_element != 0 {
        return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
    }
    let unit = &*(self_ptr as *const TapeDigizerAudioUnit);
    if (in_id as usize) >= NUM_PARAMETERS {
        return K_AUDIO_UNIT_ERR_INVALID_PARAMETER;
    }
    let val = unit.parameters[in_id as usize];
    *out_value = if in_id == PARAM_SMOOTHING {
        val * 100.0
    } else {
        val
    };
    NO_ERR
}

unsafe extern "C" fn au_set_parameter(
    self_ptr: *mut c_void,
    in_id: AudioUnitParameterID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    in_value: AudioUnitParameterValue,
    _in_buffer_offset_in_frames: UInt32,
) -> OSStatus {
    if !in_value.is_finite() {
        return K_AUDIO_UNIT_ERR_INVALID_PARAMETER_VALUE;
    }
    if in_scope != K_AUDIO_UNIT_SCOPE_GLOBAL {
        return K_AUDIO_UNIT_ERR_INVALID_SCOPE;
    }
    if in_element != 0 {
        return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
    }
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    if (in_id as usize) >= NUM_PARAMETERS {
        return K_AUDIO_UNIT_ERR_INVALID_PARAMETER;
    }

    match in_id {
        PARAM_PRESET => {
            let max_idx = unit.total_preset_count().saturating_sub(1);
            let idx = (in_value.round() as isize).clamp(0, max_idx as isize) as usize;
            unit.parameters[PARAM_PRESET as usize] = idx as f32;
            unit.present_preset_number = idx as SInt32;
            unit.apply_preset_by_index(idx);
        }
        PARAM_REDUCTION_DB => {
            unit.parameters[PARAM_REDUCTION_DB as usize] = in_value.clamp(0.0, 36.0);
        }
        PARAM_SENSITIVITY => {
            unit.parameters[PARAM_SENSITIVITY as usize] = in_value.clamp(0.5, 3.5);
        }
        PARAM_SMOOTHING => {
            let norm = if in_value > 1.0 {
                (in_value / 100.0).clamp(0.0, 0.95)
            } else {
                in_value.clamp(0.0, 0.95)
            };
            unit.parameters[PARAM_SMOOTHING as usize] = norm;
        }
        PARAM_DC_ENABLED => {
            unit.parameters[PARAM_DC_ENABLED as usize] = if in_value >= 0.5 { 1.0 } else { 0.0 };
        }
        PARAM_DC_CUTOFF_HZ => {
            unit.parameters[PARAM_DC_CUTOFF_HZ as usize] = in_value.clamp(10.0, 60.0);
        }
        PARAM_AUDITION_MODE => {
            unit.parameters[PARAM_AUDITION_MODE as usize] = in_value.round().clamp(0.0, 2.0);
        }
        PARAM_LEARN_NOISE => {
            unit.parameters[PARAM_LEARN_NOISE as usize] = if in_value >= 0.5 { 1.0 } else { 0.0 };
        }
        _ => return K_AUDIO_UNIT_ERR_INVALID_PARAMETER,
    }
    NO_ERR
}

unsafe extern "C" fn au_schedule_parameters(
    self_ptr: *mut c_void,
    in_events: *const AudioUnitParameterEvent,
    in_num_events: UInt32,
) -> OSStatus {
    if in_events.is_null() && in_num_events > 0 {
        return K_AUDIO_PARAM_ERR;
    }
    for i in 0..(in_num_events as usize) {
        let ev = *in_events.add(i);
        let val = if ev.event_type == 2 {
            ev.event_values.ramp.end_value
        } else {
            ev.event_values.immediate.value
        };
        let _ = au_set_parameter(self_ptr, ev.parameter, ev.scope, ev.element, val, 0);
    }
    NO_ERR
}

unsafe extern "C" fn au_add_property_listener(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_proc: AudioUnitPropertyListenerProc,
    in_user_data: *mut c_void,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.property_listeners.push(PropertyListenerEntry {
        prop_id: in_id,
        proc: in_proc,
        user_data: in_user_data,
    });
    NO_ERR
}

unsafe extern "C" fn au_remove_property_listener(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_proc: AudioUnitPropertyListenerProc,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.property_listeners
        .retain(|e| !(e.prop_id == in_id && e.proc as usize == in_proc as usize));
    NO_ERR
}

unsafe extern "C" fn au_remove_property_listener_with_user_data(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_proc: AudioUnitPropertyListenerProc,
    in_user_data: *mut c_void,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.property_listeners.retain(|e| {
        !(e.prop_id == in_id && e.proc as usize == in_proc as usize && e.user_data == in_user_data)
    });
    NO_ERR
}

unsafe extern "C" fn au_add_render_notify(
    self_ptr: *mut c_void,
    in_proc: AURenderCallback,
    in_user_data: *mut c_void,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.render_notify_procs.push(RenderNotifyEntry {
        proc: in_proc,
        user_data: in_user_data,
    });
    NO_ERR
}

unsafe extern "C" fn au_remove_render_notify(
    self_ptr: *mut c_void,
    in_proc: AURenderCallback,
    in_user_data: *mut c_void,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    unit.render_notify_procs
        .retain(|e| !(e.proc as usize == in_proc as usize && e.user_data == in_user_data));
    NO_ERR
}

unsafe fn process_buffer_list(
    unit: &mut TapeDigizerAudioUnit,
    in_number_frames: UInt32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let frames = in_number_frames as usize;
    let num_buffers = (*io_data).m_number_buffers as usize;
    if num_buffers == 0 {
        return K_AUDIO_PARAM_ERR;
    }
    let bufs_ptr = (*io_data).m_buffers.as_mut_ptr();

    for b in 0..num_buffers {
        let buf = &mut *bufs_ptr.add(b);
        if buf.m_data.is_null() {
            let out_ptr = if b == 0 {
                unit.scratch_out_l.as_mut_ptr()
            } else {
                unit.scratch_out_r.as_mut_ptr()
            };
            buf.m_data = out_ptr as *mut c_void;
        }
        buf.m_data_byte_size = (frames * std::mem::size_of::<f32>()) as UInt32;
    }

    if unit.bypass != 0 {
        return NO_ERR;
    }

    let ptr0 = (*bufs_ptr).m_data as *mut f32;
    for i in 0..frames {
        unit.scratch_in_l[i] = *ptr0.add(i);
    }
    if num_buffers >= 2 {
        let ptr1 = (*bufs_ptr.add(1)).m_data as *mut f32;
        for i in 0..frames {
            unit.scratch_in_r[i] = *ptr1.add(i);
        }
    } else {
        for i in 0..frames {
            unit.scratch_in_r[i] = unit.scratch_in_l[i];
        }
    }

    // Live-Learn direkt im DAW-Track (wenn Parameter "Learn Noise from Track" aktiv ist)
    let is_learning = unit.parameters[PARAM_LEARN_NOISE as usize] >= 0.5;
    if is_learning {
        if !unit.was_learning {
            unit.learn_write_count = 0;
            unit.was_learning = true;
        }
        let cap = unit.learn_buf_l.len();
        for i in 0..frames {
            let pos = unit.learn_write_count % cap;
            unit.learn_buf_l[pos] = unit.scratch_in_l[i];
            unit.learn_buf_r[pos] = unit.scratch_in_r[i];
            unit.learn_write_count += 1;
        }
        if unit.learn_write_count >= 4096 && (unit.learn_write_count % 8192 < frames) {
            let count = unit.learn_write_count.min(cap);
            let sr = unit.output_format.m_sample_rate.round().max(8000.0) as u32;
            if let Ok(learned) = NoiseProfile::learn_from_samples(
                &unit.learn_buf_l[..count],
                &unit.learn_buf_r[..count],
                sr,
                "DAW Live-Learn",
                "DAW Track",
            ) {
                unit.denoiser.set_noise_profile(Some(learned));
                unit.parameters[PARAM_PRESET as usize] = 0.0;
                unit.present_preset_number = 0;
                unit.present_preset_name = "DAW Live-Learn".to_string();
            }
        }
    } else if unit.was_learning {
        unit.was_learning = false;
        let count = unit.learn_write_count.min(unit.learn_buf_l.len());
        if count >= 2048 {
            let sr = unit.output_format.m_sample_rate.round().max(8000.0) as u32;
            if let Ok(learned) = NoiseProfile::learn_from_samples(
                &unit.learn_buf_l[..count],
                &unit.learn_buf_r[..count],
                sr,
                "DAW Live-Learn",
                "DAW Track",
            ) {
                unit.denoiser.set_noise_profile(Some(learned));
                unit.parameters[PARAM_PRESET as usize] = 0.0;
                unit.present_preset_number = 0;
                unit.present_preset_name = "DAW Live-Learn".to_string();
            }
        }
    }

    let params = unit.current_denoiser_params();
    for i in 0..frames {
        let (out_l, out_r) = unit.denoiser.process_sample(
            unit.scratch_in_l[i],
            unit.scratch_in_r[i],
            &params,
        );
        unit.scratch_out_l[i] = out_l;
        unit.scratch_out_r[i] = out_r;
    }

    for i in 0..frames {
        *ptr0.add(i) = unit.scratch_out_l[i];
    }
    if num_buffers >= 2 {
        let ptr1 = (*bufs_ptr.add(1)).m_data as *mut f32;
        for i in 0..frames {
            *ptr1.add(i) = unit.scratch_out_r[i];
        }
    }
    NO_ERR
}

unsafe extern "C" fn au_render(
    self_ptr: *mut c_void,
    io_action_flags: *mut AudioUnitRenderActionFlags,
    in_time_stamp: *const c_void,
    in_output_bus_number: UInt32,
    in_number_frames: UInt32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    if !unit.initialized {
        return K_AUDIO_UNIT_ERR_UNINITIALIZED;
    }
    if in_output_bus_number != 0 {
        return K_AUDIO_UNIT_ERR_INVALID_ELEMENT;
    }
    if in_number_frames > unit.max_frames_per_slice {
        return K_AUDIO_UNIT_ERR_TOO_MANY_FRAMES_TO_PROCESS;
    }
    if io_data.is_null() || in_time_stamp.is_null() {
        return K_AUDIO_PARAM_ERR;
    }

    let mut local_flags: AudioUnitRenderActionFlags = 0;
    let flags_ptr = if io_action_flags.is_null() {
        &mut local_flags as *mut AudioUnitRenderActionFlags
    } else {
        io_action_flags
    };

    for notify in &unit.render_notify_procs {
        let mut pre_flags = *flags_ptr | K_AUDIO_UNIT_RENDER_ACTION_PRE_RENDER;
        (notify.proc)(
            notify.user_data,
            &mut pre_flags,
            in_time_stamp,
            in_output_bus_number,
            in_number_frames,
            io_data,
        );
    }

    let frames = in_number_frames as usize;
    let num_buffers = (*io_data).m_number_buffers as usize;
    let bufs_ptr = (*io_data).m_buffers.as_mut_ptr();
    for b in 0..num_buffers {
        let buf = &mut *bufs_ptr.add(b);
        if buf.m_data.is_null() {
            let scratch_ptr = if b == 0 {
                unit.scratch_in_l.as_mut_ptr()
            } else {
                unit.scratch_in_r.as_mut_ptr()
            };
            buf.m_data = scratch_ptr as *mut c_void;
        }
        buf.m_data_byte_size = (frames * std::mem::size_of::<f32>()) as UInt32;
    }

    let pull_status = if let Some(cb) = unit.render_callback.input_proc {
        cb(
            unit.render_callback.input_proc_ref_con,
            flags_ptr,
            in_time_stamp,
            0,
            in_number_frames,
            io_data,
        )
    } else if !unit.connection.source_audio_unit.is_null() {
        AudioUnitRender(
            unit.connection.source_audio_unit,
            flags_ptr,
            in_time_stamp,
            unit.connection.source_output_number,
            in_number_frames,
            io_data,
        )
    } else {
        K_AUDIO_UNIT_ERR_NO_CONNECTION
    };

    if pull_status != NO_ERR {
        unit.last_render_error = pull_status;
        return pull_status;
    }

    let status = process_buffer_list(unit, in_number_frames, io_data);
    unit.last_render_error = status;

    for notify in &unit.render_notify_procs {
        let mut post_flags = *flags_ptr | K_AUDIO_UNIT_RENDER_ACTION_POST_RENDER;
        (notify.proc)(
            notify.user_data,
            &mut post_flags,
            in_time_stamp,
            in_output_bus_number,
            in_number_frames,
            io_data,
        );
    }

    status
}

unsafe extern "C" fn au_process(
    self_ptr: *mut c_void,
    _io_action_flags: *mut AudioUnitRenderActionFlags,
    in_time_stamp: *const c_void,
    in_number_frames: UInt32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let unit = &mut *(self_ptr as *mut TapeDigizerAudioUnit);
    if !unit.initialized {
        return K_AUDIO_UNIT_ERR_UNINITIALIZED;
    }
    if in_number_frames > unit.max_frames_per_slice {
        return K_AUDIO_UNIT_ERR_TOO_MANY_FRAMES_TO_PROCESS;
    }
    if io_data.is_null() || in_time_stamp.is_null() {
        return K_AUDIO_PARAM_ERR;
    }
    process_buffer_list(unit, in_number_frames, io_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audiounit_lifecycle_render_and_live_learn() {
        unsafe {
            let desc = AudioComponentDescription {
                component_type: AU_TYPE_EFFECT as UInt32,
                component_sub_type: AU_SUBTYPE_DENOISER as UInt32,
                component_manufacturer: AU_MANUFACTURER_TAPEDIGIZER as UInt32,
                component_flags: 0,
                component_flags_mask: 0,
            };
            let iface = TapeDigizerDenoiserFactory(&desc);
            assert!(!iface.is_null());
            let self_ptr = iface as *mut c_void;

            assert_eq!(((*iface).open)(self_ptr, self_ptr), NO_ERR);
            assert_eq!(au_initialize(self_ptr), NO_ERR);

            // Latency property check
            let mut latency: Float64 = 0.0;
            let mut size = std::mem::size_of::<Float64>() as UInt32;
            assert_eq!(
                au_get_property(
                    self_ptr,
                    K_AUDIO_UNIT_PROPERTY_LATENCY,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &mut latency as *mut Float64 as *mut c_void,
                    &mut size,
                ),
                NO_ERR
            );
            assert!((latency - (STFT_LATENCY_SAMPLES as f64 / 44100.0)).abs() < 1e-6);

            // Enable Live-Learn in DAW and process 4096 frames of simulated tape noise + DC
            assert_eq!(
                au_set_parameter(self_ptr, PARAM_LEARN_NOISE, K_AUDIO_UNIT_SCOPE_GLOBAL, 0, 1.0, 0),
                NO_ERR
            );

            #[repr(C)]
            struct StereoBufferList {
                m_number_buffers: UInt32,
                m_buffers: [AudioBuffer; 2],
            }

            let frames = 512usize;
            let mut buf_l = vec![0.02f32; frames];
            let mut buf_r = vec![-0.02f32; frames];
            let dummy_ts = [0u64; 8];

            for hop in 0..8 {
                for i in 0..frames {
                    let phase = (hop * frames + i) as f32 * 0.13;
                    buf_l[i] = 0.015 + 0.005 * phase.sin();
                    buf_r[i] = -0.012 + 0.005 * phase.cos();
                }
                let mut abl = StereoBufferList {
                    m_number_buffers: 2,
                    m_buffers: [
                        AudioBuffer {
                            m_number_channels: 1,
                            m_data_byte_size: (frames * 4) as UInt32,
                            m_data: buf_l.as_mut_ptr() as *mut c_void,
                        },
                        AudioBuffer {
                            m_number_channels: 1,
                            m_data_byte_size: (frames * 4) as UInt32,
                            m_data: buf_r.as_mut_ptr() as *mut c_void,
                        },
                    ],
                };
                let mut flags: AudioUnitRenderActionFlags = 0;
                assert_eq!(
                    au_process(
                        self_ptr,
                        &mut flags,
                        dummy_ts.as_ptr() as *const c_void,
                        frames as UInt32,
                        &mut abl as *mut StereoBufferList as *mut AudioBufferList,
                    ),
                    NO_ERR
                );
            }

            // Finish Live-Learn
            assert_eq!(
                au_set_parameter(self_ptr, PARAM_LEARN_NOISE, K_AUDIO_UNIT_SCOPE_GLOBAL, 0, 0.0, 0),
                NO_ERR
            );

            // Save & restore ClassInfo state dictionary
            let mut dict: CFDictionaryRef = ptr::null();
            let mut dict_size = std::mem::size_of::<CFDictionaryRef>() as UInt32;
            assert_eq!(
                au_get_property(
                    self_ptr,
                    K_AUDIO_UNIT_PROPERTY_CLASS_INFO,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &mut dict as *mut CFDictionaryRef as *mut c_void,
                    &mut dict_size,
                ),
                NO_ERR
            );
            assert!(!dict.is_null());
            assert_eq!(
                au_set_property(
                    self_ptr,
                    K_AUDIO_UNIT_PROPERTY_CLASS_INFO,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    &dict as *const CFDictionaryRef as *const c_void,
                    dict_size,
                ),
                NO_ERR
            );
            CFRelease(dict);

            assert_eq!(au_uninitialize(self_ptr), NO_ERR);
            assert_eq!(((*iface).close)(self_ptr), NO_ERR);
        }
    }
}
