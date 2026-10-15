//! P1-3 FormatGate — Master Spec §3.2 / TDD §3.2
//!
//! 两阶段门控:
//!   Stage 1 (零字节元数据门控, G1A/G1B/G2/G3/G4) — 免费拦截, 绝不读取 payload
//!   Stage 2 (双层体积分级, G5)                   — 仅对通过 Stage1 的对象读取字节数

/// 复制事件的元数据快照 (来自 NSPasteboard item.types / changeCount)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardMetadata {
    /// 全部 item 声明的 UTI
    pub all_types: Vec<String>,
    /// item 数量 (pasteboardItems)
    pub item_count: usize,
    /// 该 item 的 UTI 集合
    pub item_types: Vec<String>,
    /// 是否含 public.file-url / NSFilenamesPboardType
    pub has_file_url: bool,
    /// 是否含 public.tiff / public.png (纯图片信号)
    pub has_image: bool,
    /// 是否含 html/rtf/plainText 文本通道
    pub has_text: bool,
    /// 是否命中 Legacy 垃圾格式 (TDD G4: 长期污染源)
    pub has_legacy_junk: bool,
    /// 声明为私有应用格式 (com.xxx.yyy 私有 UTI, 非 public.*)
    pub has_private_types: bool,
}

/// Stage 1 门控判定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage1Verdict {
    /// G1A: 私有对象拦截 (如 Drafts/Notion 私有 UTI)
    PrivateObject,
    /// G1B: 纯图片拦截; 例外: Word 伴生 TIFF (html/rtf + tiff 组合) 放行
    PureImage,
    /// G2: 多条目拦截 (Excel 多 cell / Finder 多文件)
    MultiItem,
    /// G3: 文件复制拦截
    FileCopy,
    /// G4: Legacy 垃圾格式拦截
    LegacyJunk,
    /// 通过 Stage 1
    Pass,
}

/// Stage 2 体积等级 (TDD §3.2 G5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeTier {
    /// <= 64KB: 全量接管 (三通道供给)
    FullTakeover,
    /// 64KB ~ 256KB: 轻量接管 (仅 plainText)
    LightPlainOnly,
    /// > 256KB: 拒绝接管 (纯转发)
    Refuse,
}

/// Stage 2 输入: 各通道字节数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeBytes {
    pub html: usize,
    pub rtf: usize,
    pub plain: usize,
}

impl VolumeBytes {
    /// 计入体积的总字节 (图片不计: G1B 已拦截纯图片, Word 伴生 TIFF 不参与体积预算)
    pub fn charged(&self) -> usize {
        self.html.saturating_add(self.rtf).saturating_add(self.plain)
    }
}

/// 免费拦截阈值 (Master Spec §3.2)
pub const STAGE2_FULL_MAX_BYTES: usize = 64 * 1024;
pub const STAGE2_REFUSE_MIN_BYTES: usize = 256 * 1024;

/// Legacy 垃圾格式清单 (TDD G4)
const LEGACY_JUNK_TYPES: &[&str] = &[
    "com.apple.traditional-mac-plain-text",
    "CorePasteboardFlavorType",
    "NeXT Rich Text Format v0.9 pasteboard type",
    "public.utf16-external-plain-text", // 别名不作为独立通道, 但主 type 缺失时视为 legacy
];

/// 私有 UTI 前缀 (非 public. / dyn. 即视为应用私有格式)
fn is_private_ut(uti: &str) -> bool {
    !(uti.starts_with("public.")
        || uti.starts_with("dyn.")
        || uti.starts_with("com.apple.")
        || uti.starts_with("org.")
        || uti.starts_with("net."))
}

impl ClipboardMetadata {
    /// Stage 1: 零字节元数据门控
    pub fn stage1(&self) -> Stage1Verdict {
        // G2 多条目: 优先级最高 (Excel 多 cell 从不触发 G1B 纯图片)
        if self.item_count > 1 {
            return Stage1Verdict::MultiItem;
        }
        // G3 文件复制
        if self.has_file_url {
            return Stage1Verdict::FileCopy;
        }
        // G4 Legacy 垃圾格式
        if self.has_legacy_junk {
            return Stage1Verdict::LegacyJunk;
        }
        // G1A 私有对象拦截: 有私有 UTI 且无文本通道 (有文本则为正常内容, 允许接管)
        if self.has_private_types && !self.has_text {
            return Stage1Verdict::PrivateObject;
        }
        // G1B 纯图片拦截; 例外: Word 伴生 TIFF = 图片 + 文本通道组合 -> 放行
        if self.has_image && !self.has_text {
            return Stage1Verdict::PureImage;
        }
        Stage1Verdict::Pass
    }
}

/// Stage 2: 双层体积分级
pub fn stage2(vol: VolumeBytes) -> VolumeTier {
    let total = vol.charged();
    if total > STAGE2_REFUSE_MIN_BYTES {
        VolumeTier::Refuse
    } else if total > STAGE2_FULL_MAX_BYTES {
        VolumeTier::LightPlainOnly
    } else {
        VolumeTier::FullTakeover
    }
}

/// 从 UTI 列表构建元数据 (供 clipboard_owner 调用)
pub fn metadata_from_types(
    item_types: Vec<String>,
    all_types: Vec<String>,
    item_count: usize,
) -> ClipboardMetadata {
    let has_file_url = all_types.iter().any(|t| {
        t == "public.file-url" || t == "com.apple.NSilenamesPboardType" || t == "public.url"
    });
    let has_image = all_types
        .iter()
        .any(|t| t == "public.tiff" || t == "public.png" || t == "com.apple.pict");
    let has_text = all_types.iter().any(|t| {
        t == "public.utf8-plain-text"
            || t == "public.plain-text"
            || t == "public.html"
            || t == "public.rtf"
            || t == "public.rtfd"
    });
    let has_legacy_junk = all_types
        .iter()
        .any(|t| LEGACY_JUNK_TYPES.iter().any(|j| t.contains(j)));
    let has_private_types = item_types.iter().any(|t| is_private_ut(t));
    ClipboardMetadata {
        all_types,
        item_count,
        item_types,
        has_file_url,
        has_image,
        has_text,
        has_legacy_junk,
        has_private_types,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(types: &[&str], item_count: usize) -> ClipboardMetadata {
        let owned: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        metadata_from_types(owned.clone(), owned, item_count)
    }

    // —— Stage 1 ——
    #[test]
    fn g1a_private_object_blocked() {
        let m = meta(&["com.todesktop.draft", "com.culturedcode.ThingsTask"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::PrivateObject);
    }

    #[test]
    fn g1b_pure_image_blocked() {
        let m = meta(&["public.tiff", "public.png"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::PureImage);
    }

    #[test]
    fn g1b_word_companion_tiff_passes() {
        // Word 复制: 富文本通道 + 伴生 TIFF -> 放行富文本路径
        let m = meta(&["public.html", "public.rtf", "public.utf8-plain-text", "public.tiff"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::Pass);
    }

    #[test]
    fn g2_multi_item_blocked() {
        let m = meta(&["public.utf8-plain-text"], 3);
        assert_eq!(m.stage1(), Stage1Verdict::MultiItem);
    }

    #[test]
    fn g3_file_copy_blocked() {
        let m = meta(&["public.file-url", "public.utf8-plain-text"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::FileCopy);
    }

    #[test]
    fn g4_legacy_junk_blocked() {
        let m = meta(&["NeXT Rich Text Format v0.9 pasteboard type", "public.utf8-plain-text"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::LegacyJunk);
    }

    #[test]
    fn private_type_with_text_passes() {
        // 有文本通道的私有格式组合 = 正常内容
        let m = meta(&["com.electron.lark.internal", "public.utf8-plain-text"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::Pass);
    }

    #[test]
    fn plain_text_passes_stage1() {
        let m = meta(&["public.utf8-plain-text"], 1);
        assert_eq!(m.stage1(), Stage1Verdict::Pass);
    }

    // —— Stage 2 ——
    #[test]
    fn stage2_full_takeover_under_64k() {
        let v = VolumeBytes { html: 20_000, rtf: 15_000, plain: 20_000 };
        assert_eq!(stage2(v), VolumeTier::FullTakeover);
    }

    #[test]
    fn stage2_light_between_64k_and_256k() {
        let v = VolumeBytes { html: 100_000, rtf: 50_000, plain: 10_000 };
        assert_eq!(stage2(v), VolumeTier::LightPlainOnly);
    }

    #[test]
    fn stage2_refuse_over_256k() {
        let v = VolumeBytes { html: 300_000, rtf: 0, plain: 10 };
        assert_eq!(stage2(v), VolumeTier::Refuse);
    }

    #[test]
    fn stage2_boundary_64k_is_full() {
        let v = VolumeBytes { html: 64 * 1024, rtf: 0, plain: 0 };
        assert_eq!(stage2(v), VolumeTier::FullTakeover);
    }

    #[test]
    fn stage2_boundary_256k_is_light() {
        let v = VolumeBytes { html: 256 * 1024, rtf: 0, plain: 0 };
        assert_eq!(stage2(v), VolumeTier::LightPlainOnly);
    }
}
