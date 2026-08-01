//! Settings 模型。
//!
//! Phase 1：theme + scanDirs；Phase 3：audioOutput（WASAPI/ReplayGain/gapless）。

use serde::{Deserialize, Serialize};

/// 主题。序列化为契约的 `"dark" | "light" | "system"`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

/// 音频输出后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioBackend {
    /// 系统默认（走混音器，兼容性最好）。
    #[default]
    System,
    /// WASAPI 共享模式（Windows，低延迟，与其他应用共存）。
    WasapiShared,
    /// WASAPI 独占模式（Windows，绕过混音器 HiFi 直出）。
    WasapiExclusive,
}

/// ReplayGain 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplayGainMode {
    /// 不应用 ReplayGain。
    #[default]
    Off,
    /// 按曲目增益。
    Track,
    /// 按专辑增益。
    Album,
}

/// 音频输出配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutputConfig {
    /// 输出后端。
    #[serde(default)]
    pub backend: AudioBackend,
    /// 输出设备名称（None = 系统默认设备）。
    #[serde(default)]
    pub device: Option<String>,
    /// 无缝播放（gapless）。
    #[serde(default = "default_true")]
    pub gapless: bool,
    /// ReplayGain 模式。
    #[serde(default)]
    pub replay_gain: ReplayGainMode,
    /// 无 RG 标签时启用 loudnorm 滤镜做响度归一化。
    #[serde(default)]
    pub loudnorm_fallback: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for AudioOutputConfig {
    fn default() -> Self {
        AudioOutputConfig {
            backend: AudioBackend::System,
            device: None,
            gapless: true,
            replay_gain: ReplayGainMode::Off,
            loudnorm_fallback: false,
        }
    }
}

/// 应用设置。`update_settings` 为全量替换语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub theme: Theme,
    /// 媒体库扫描目录（绝对路径字符串）。
    #[serde(default)]
    pub scan_dirs: Vec<String>,
    /// 音频输出配置（Phase 3）。
    #[serde(default)]
    pub audio_output: AudioOutputConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Theme::Dark,
            scan_dirs: Vec::new(),
            audio_output: AudioOutputConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_contract() {
        let s = Settings::default();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["scanDirs"], serde_json::json!([]));
        assert_eq!(v["audioOutput"]["backend"], "system");
        assert_eq!(v["audioOutput"]["gapless"], true);
        assert_eq!(v["audioOutput"]["replayGain"], "off");
    }

    #[test]
    fn roundtrip_and_missing_fields_fall_back_to_defaults() {
        let s = Settings {
            theme: Theme::System,
            scan_dirs: vec!["D:/Music".to_string()],
            audio_output: AudioOutputConfig {
                backend: AudioBackend::WasapiExclusive,
                device: Some("DAC-1".to_string()),
                gapless: true,
                replay_gain: ReplayGainMode::Album,
                loudnorm_fallback: true,
            },
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back, s);

        // 缺字段时使用默认值，保证旧数据可向前兼容
        let partial: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(partial, Settings::default());

        // Phase 1 旧数据（无 audioOutput）向前兼容
        let old: Settings =
            serde_json::from_str(r#"{"theme":"light","scanDirs":["D:/M"]}"#).unwrap();
        assert_eq!(old.audio_output, AudioOutputConfig::default());
    }

    #[test]
    fn invalid_theme_is_rejected() {
        assert!(serde_json::from_str::<Settings>(r#"{"theme":"blue"}"#).is_err());
    }

    #[test]
    fn audio_backend_serialization() {
        assert_eq!(
            serde_json::to_value(AudioBackend::WasapiShared).unwrap(),
            "wasapi-shared"
        );
        assert_eq!(
            serde_json::to_value(AudioBackend::WasapiExclusive).unwrap(),
            "wasapi-exclusive"
        );
        let b: AudioBackend = serde_json::from_str("\"wasapi-exclusive\"").unwrap();
        assert_eq!(b, AudioBackend::WasapiExclusive);
    }
}
