use serde::{Deserialize, Serialize};

/// RGBA/RGB 색상 토큰 (`#RRGGBB` 16진수 및 `[u8; 3]` 표현).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorToken {
    pub name_kr: &'static str,
    pub ident: &'static str,
    pub hex: &'static str,
    pub rgb: [u8; 3],
}

impl ColorToken {
    pub const fn new(
        name_kr: &'static str,
        ident: &'static str,
        hex: &'static str,
        r: u8,
        g: u8,
        b: u8,
    ) -> Self {
        Self {
            name_kr,
            ident,
            hex,
            rgb: [r, g, b],
        }
    }
}

/// `STRATA · 디자인 기준` (6페이지 PDF) 및 `UI_RULES.md` R2 공식 색상 토큰.
/// "종이 위에 먹으로 쓰고, 흙색은 화면마다 한 번"
pub mod tokens {
    use super::ColorToken;

    // 1. 바탕과 면 · 글자 (WCAG AAA/AA 명암비 검증 완료)
    pub const GROUND: ColorToken = ColorToken::new("종이", "Ground", "#FAF9F5", 0xFA, 0xF9, 0xF5);
    pub const WASH: ColorToken = ColorToken::new("옅은 종이", "Wash", "#F5F4ED", 0xF5, 0xF4, 0xED);
    pub const DESK: ColorToken = ColorToken::new("눌린 면", "Desk", "#F0EEE6", 0xF0, 0xEE, 0xE6);
    pub const SURFACE: ColorToken = ColorToken::new("흰 면", "Surface", "#FFFFFF", 0xFF, 0xFF, 0xFF);
    pub const INK: ColorToken = ColorToken::new("먹", "Ink", "#141413", 0x14, 0x14, 0x13);
    pub const MUTED: ColorToken = ColorToken::new("보조 글자", "Muted", "#5E5D59", 0x5E, 0x5D, 0x59);
    pub const FAINT: ColorToken = ColorToken::new("흐린 글자", "Faint", "#73726C", 0x73, 0x72, 0x6C);
    pub const DISABLED: ColorToken =
        ColorToken::new("꺼진 글자", "Disabled", "#B0AEA5", 0xB0, 0xAE, 0xA5);
    pub const LINE: ColorToken = ColorToken::new("구분선", "Line", "#DEDCD1", 0xDE, 0xDC, 0xD1);
    pub const EDGE: ColorToken = ColorToken::new("테두리", "Edge", "#C2C0B6", 0xC2, 0xC0, 0xB6);

    // 2. 강조와 상태 · 도판 전용 선 색상
    pub const CLAY_FOCUS: ColorToken =
        ColorToken::new("흙(초점)", "ClayFocus", "#D97757", 0xD9, 0x77, 0x57);
    pub const ACTION: ColorToken =
        ColorToken::new("짙은 흙(큰 행동)", "Action", "#B5573A", 0xB5, 0x57, 0x3A);
    pub const ACTION_HOVER: ColorToken =
        ColorToken::new("짙은 흙 호버", "ActionHover", "#A44D32", 0xA4, 0x4D, 0x32);
    pub const CLAY_INK: ColorToken =
        ColorToken::new("흙빛 글자", "ClayInk", "#9C4A2F", 0x9C, 0x4A, 0x2F);
    pub const CLAY_WASH: ColorToken =
        ColorToken::new("흙 물(켜진 타일)", "ClayWash", "#F4E3DA", 0xF4, 0xE3, 0xDA);
    pub const HAND: ColorToken =
        ColorToken::new("손에 든 도구", "Hand", "#30302E", 0x30, 0x30, 0x2E);
    pub const DONE_BG: ColorToken =
        ColorToken::new("통과 바탕", "DoneBg", "#E6EEDD", 0xE6, 0xEE, 0xDD);
    pub const DONE_INK: ColorToken =
        ColorToken::new("통과 글자", "Done", "#3F6B31", 0x3F, 0x6B, 0x31);
    pub const WARN_BG: ColorToken =
        ColorToken::new("주의 바탕", "WarnBg", "#F6ECC8", 0xF6, 0xEC, 0xC8);
    pub const WARN_INK: ColorToken =
        ColorToken::new("주의 글자", "Caution", "#7A5A00", 0x7A, 0x5A, 0x00);
    pub const ERR_BG: ColorToken =
        ColorToken::new("오류 바탕", "ErrBg", "#F3DEDA", 0xF3, 0xDE, 0xDA);
    pub const ERR_INK: ColorToken =
        ColorToken::new("오류 글자", "ErrInk", "#9E2A2B", 0x9E, 0x2A, 0x2B);
    pub const SELECT_BLUE: ColorToken =
        ColorToken::new("도형 선택", "SelectBlue", "#2A78D6", 0x2A, 0x78, 0xD6);
    pub const PROFILE_RED: ColorToken =
        ColorToken::new("실측 단면선", "ProfileRed", "#FF0000", 0xFF, 0x00, 0x00);
    pub const GRID_1MM: ColorToken =
        ColorToken::new("방안 1mm 세선", "Grid1mm", "#EDEBE2", 0xED, 0xEB, 0xE2);
    pub const GRID_10MM: ColorToken =
        ColorToken::new("방안 10mm 주선", "Grid10mm", "#D5D2C6", 0xD5, 0xD2, 0xC6);

    pub const ALL_ALLOWED_TOKENS: &[ColorToken] = &[
        GROUND,
        WASH,
        DESK,
        SURFACE,
        INK,
        MUTED,
        FAINT,
        DISABLED,
        LINE,
        EDGE,
        CLAY_FOCUS,
        ACTION,
        ACTION_HOVER,
        CLAY_INK,
        CLAY_WASH,
        HAND,
        DONE_BG,
        DONE_INK,
        WARN_BG,
        WARN_INK,
        ERR_BG,
        ERR_INK,
        SELECT_BLUE,
        PROFILE_RED,
        GRID_1MM,
        GRID_10MM,
    ];
}

/// `UI_RULES.md` R1 5단 화면 골격 및 `STRATA · 디자인 기준` 창·타일·글꼴 치수 규격.
pub mod metrics {
    // 1. R1 5단 화면 레이아웃 고정 높이·폭 (px)
    pub const TAB_BAR_HEIGHT: u32 = 34;
    pub const RIBBON_HEIGHT: u32 = 98;
    pub const CTX_BAR_HEIGHT: u32 = 36;
    pub const STATUS_BAR_HEIGHT: u32 = 30;
    pub const SLOT_LIST_WIDTH: u32 = 176;
    pub const SHEET_HEADER_HEIGHT: u32 = 32;
    pub const METRIC_STRIP_HEIGHT: u32 = 28;
    pub const RIM_GAP_BAR_HEIGHT: u32 = 36;

    // 2. 작업 공간 기본 분할 비율 (3D 보조창 27% : 실측 도판 73%, Tab 누르면 도판 100%)
    pub const VIEW3D_DEFAULT_RATIO: f32 = 0.27;
    pub const SHEET_DEFAULT_RATIO: f32 = 0.73;
    pub const SHEET_MAXIMIZED_RATIO: f32 = 1.00;
    pub const VIEW3D_MIN_WIDTH: u32 = 240;
    pub const VIEW3D_STEP3_WIDTH: u32 = 330;

    // 3. 리본 타일 & 버튼·입력칸·칩 규격 (STRATA PDF p.6)
    pub const RIBBON_TILE_SIZE: u32 = 50;
    pub const RIBBON_TILE_RADIUS: u32 = 12;
    pub const RIBBON_TILE_ICON_SIZE: u32 = 32;
    pub const BUTTON_HEIGHT: u32 = 34;
    pub const BUTTON_SMALL_HEIGHT: u32 = 23;
    pub const BUTTON_GAP: u32 = 8;
    pub const INPUT_HEIGHT: u32 = 34;
    pub const STATUS_CHIP_HEIGHT: u32 = 22;

    // 4. 창의 뼈대와 5가지 표준 크기 (STRATA PDF p.3, p.6)
    pub const DIALOG_HEADER_HEIGHT: u32 = 32;
    pub const DIALOG_BODY_PADDING: u32 = 24;
    pub const DIALOG_ROW_GAP: u32 = 16;
    pub const DIALOG_LABEL_WIDTH: u32 = 96;
    pub const DIALOG_LABEL_GAP: u32 = 16;
    pub const DIALOG_FOOTER_HEIGHT: u32 = 58;

    pub const DIALOG_S_CONFIRM_W: u32 = 420;
    pub const DIALOG_S_ERROR_W: u32 = 460;
    pub const DIALOG_PROGRESS_W: u32 = 480;
    pub const DIALOG_M_SETTINGS_W: u32 = 520;
    pub const DIALOG_L_W: u32 = 860;
    pub const DIALOG_L_VERIFY_H: u32 = 720;
    pub const DIALOG_L_PRINT_H: u32 = 560;
    pub const DIALOG_XL_W: u32 = 1280;
    pub const DIALOG_XL_H: u32 = 860;

    // 5. 시작(스플래시) 화면 규격 (STRATA PDF p.4)
    pub const SPLASH_WIDTH: u32 = 700;
    pub const SPLASH_HEIGHT: u32 = 400;
    pub const SPLASH_DURATION_SEC: f32 = 3.0;

    // 6. 타이포그래피 8단계 크기 (px)
    pub const FONT_T1_HERO_SERIF: f32 = 36.0;
    pub const FONT_T2_CARD_SERIF: f32 = 22.0;
    pub const FONT_T3_DIALOG_LEAD: f32 = 15.0;
    pub const FONT_T4_BODY_BUTTON: f32 = 13.0;
    pub const FONT_T5_MUTED_CHIP: f32 = 12.0;
    pub const FONT_T6_GROUP_LABEL: f32 = 11.0;
    pub const FONT_T7_SLOT_ITEM: f32 = 11.5;
    pub const FONT_T8_MONO_METRIC: f32 = 12.0;
}
