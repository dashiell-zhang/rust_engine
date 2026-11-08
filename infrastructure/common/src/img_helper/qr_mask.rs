use qrcode::{
    Color, EcLevel,
    bits::Bits,
    canvas::{Canvas, MaskPattern},
    ec,
    types::QrError,
};

/// 不含留白的标准二维码矩阵。
pub(super) struct QrMatrix {
    width: usize,
    modules: Vec<Color>,
}

impl QrMatrix {
    pub(super) fn width(&self) -> usize {
        self.width
    }

    pub(super) fn is_dark(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.width + x] == Color::Dark
    }
}

/// 复用纠错和模块布局，按 SkiaSharp.QrCode 1.2.0 的评分选择掩码。
pub(super) fn select_mask(bits: Bits) -> Result<QrMatrix, QrError> {
    let version = bits.version();
    let width = version.width() as usize;
    let (data, correction) = ec::construct_codewords(&bits.into_bytes(), version, EcLevel::L)?;
    let mut base = Canvas::new(version, EcLevel::L);
    base.draw_all_functional_patterns();
    base.draw_data(&data, &correction);

    let masks = [
        MaskPattern::Checkerboard,
        MaskPattern::HorizontalLines,
        MaskPattern::VerticalLines,
        MaskPattern::DiagonalLines,
        MaskPattern::LargeCheckerboard,
        MaskPattern::Fields,
        MaskPattern::Diamonds,
        MaskPattern::Meadow,
    ];
    let mut best_score = u32::MAX;
    let mut best = QrMatrix {
        width,
        modules: Vec::new(),
    };
    for mask in masks {
        let mut candidate = base.clone();
        // apply_mask 同时写入该候选的格式信息；版本信息已在功能区中。
        candidate.apply_mask(mask);
        let matrix = QrMatrix {
            width,
            modules: candidate.into_colors(),
        };
        let score = penalty(&matrix);
        // 源实现只在严格更小时替换，同分保留最小掩码编号。
        if score < best_score {
            best_score = score;
            best = matrix;
        }
    }
    Ok(best)
}

fn penalty(matrix: &QrMatrix) -> u32 {
    let width = matrix.width;
    let mut score = 0;
    for index in 0..width {
        score += line_penalty((0..width).map(|x| matrix.is_dark(x, index)));
        score += line_penalty((0..width).map(|y| matrix.is_dark(index, y)));
    }
    // 每一个同色 2×2 方块均计分，允许重叠。
    for y in 0..width - 1 {
        for x in 0..width - 1 {
            let color = matrix.is_dark(x, y);
            if matrix.is_dark(x + 1, y) == color
                && matrix.is_dark(x, y + 1) == color
                && matrix.is_dark(x + 1, y + 1) == color
            {
                score += 3;
            }
        }
    }
    // 与源一致使用 double 计算百分比，再取两侧 5% 档位的较小偏离。
    let dark = matrix
        .modules
        .iter()
        .filter(|&&color| color == Color::Dark)
        .count();
    let percent = (dark as f64 / (width * width) as f64) * 100.0;
    let lower = (((percent / 5.0).floor() as i32) * 5 - 50).unsigned_abs() / 5;
    let upper = (((percent / 5.0).ceil() as i32) * 5 - 50).unsigned_abs() / 5;
    score + lower.min(upper) * 10
}

/// 连续同色和定位图案相似度只检查矩阵内的完整窗口，不借用外部留白。
fn line_penalty(line: impl Iterator<Item = bool>) -> u32 {
    let mut previous = false;
    let mut run = 0;
    let mut window = 0_u16;
    let mut score = 0;
    for (index, dark) in line.enumerate() {
        if run == 0 || previous != dark {
            previous = dark;
            run = 1;
        } else {
            run += 1;
            if run == 5 {
                score += 3;
            } else if run > 5 {
                score += 1;
            }
        }
        window = ((window << 1) | u16::from(dark)) & 0x7ff;
        if index >= 10 && matches!(window, 0b00001011101 | 0b10111010000) {
            score += 40;
        }
    }
    score
}
