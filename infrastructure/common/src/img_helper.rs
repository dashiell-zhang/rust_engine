use font8x8::{
    BASIC_FONTS, BLOCK_FONTS, BOX_FONTS, GREEK_FONTS, HIRAGANA_FONTS, LATIN_FONTS, MISC_FONTS,
    SGA_FONTS, UnicodeFonts,
};
use image::{
    DynamicImage, ImageBuffer, ImageFormat, Luma, Rgba, RgbaImage,
    imageops::{FilterType, crop_imm},
};
use qrcode::QrCode;
use std::io::Cursor;
use std::path::Path;

/// 生成二维码
pub fn get_qr_code(text: &str) -> Result<Vec<u8>, image::ImageError> {
    let code = QrCode::new(text.as_bytes())
        .map_err(|error| image::ImageError::IoError(std::io::Error::other(error.to_string())))?;
    let image: ImageBuffer<Luma<u8>, Vec<u8>> =
        code.render::<Luma<u8>>().min_dimensions(500, 500).build();
    let dynamic = DynamicImage::ImageLuma8(image);
    encode_png(dynamic)
}

/// 从图片截取部分区域
pub fn screenshot(
    from_image_path: impl AsRef<Path>,
    offset_x: u32,
    offset_y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, image::ImageError> {
    let image = image::open(from_image_path)?;
    if width == 0 || height == 0 {
        return Err(invalid_image_input("截图宽高必须大于 0"));
    }

    let Some(end_x) = offset_x.checked_add(width) else {
        return Err(invalid_image_input("截图横向范围无效"));
    };
    let Some(end_y) = offset_y.checked_add(height) else {
        return Err(invalid_image_input("截图纵向范围无效"));
    };
    if end_x > image.width() || end_y > image.height() {
        return Err(invalid_image_input("截图范围超出图片尺寸"));
    }

    let cropped = crop_imm(&image, offset_x, offset_y, width, height).to_image();
    encode_png(DynamicImage::ImageRgba8(cropped))
}

/// 获取图像验证码
pub fn get_verify_code(text: &str) -> Result<Vec<u8>, image::ImageError> {
    let characters = validate_verify_code_text(text)?;
    let width = 128_u32.max(30 + characters.len() as u32 * 32);
    let height = 45;
    let mut image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    for _ in 0..((width * height) / 12) {
        let x = rand::random_range(0..width);
        let y = rand::random_range(0..height);
        image.put_pixel(
            x,
            y,
            Rgba([
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                255,
            ]),
        );
    }

    for (index, glyph) in characters.into_iter().enumerate() {
        draw_glyph(
            &mut image,
            glyph,
            15 + index as u32 * 32,
            6,
            Rgba([
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                255,
            ]),
        );
    }

    encode_png(DynamicImage::ImageRgba8(image))
}

/// 校验图片验证码文本并解析字形
fn validate_verify_code_text(text: &str) -> Result<Vec<[u8; 8]>, image::ImageError> {
    let characters = text.chars().collect::<Vec<_>>();
    if text.trim().is_empty() {
        return Err(invalid_image_input("验证码文本不能为空"));
    }
    if characters.len() > 8 {
        return Err(invalid_image_input("验证码文本不能超过 8 个字符"));
    }

    characters
        .into_iter()
        .map(|character| {
            resolve_glyph(character)
                .ok_or_else(|| invalid_image_input("验证码包含当前字体不支持的字符"))
        })
        .collect()
}

/// 将图片字节按指定宽高缩放并保持原始格式
pub fn resize_image(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, image::ImageError> {
    // 未指定缩放尺寸时直接返回原始字节
    if width == 0 && height == 0 {
        return Ok(data.to_vec());
    }

    let format = image::guess_format(data)?;
    let image = image::load_from_memory(data)?;
    let (target_width, target_height) = calculate_resize_dimensions(&image, width, height);

    // 与 NetEngine 对齐：目标宽高都不小于原图时直接返回原图，避免无意义放大。
    if target_width >= image.width() && target_height >= image.height() {
        return Ok(data.to_vec());
    }

    // 尺寸已提前按等比规则算好，这里直接输出目标尺寸
    let resized = image.resize_exact(target_width, target_height, FilterType::Lanczos3);
    encode_image(resized, format)
}

/// 编码为 PNG
fn encode_png(image: DynamicImage) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

/// 构造图片输入错误
fn invalid_image_input(message: &str) -> image::ImageError {
    image::ImageError::IoError(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

/// 按目标框计算等比缩放后的尺寸
fn calculate_resize_dimensions(image: &DynamicImage, width: u32, height: u32) -> (u32, u32) {
    let original_width = image.width().max(1);
    let original_height = image.height().max(1);

    // 仅指定高度时按原始宽高比推导宽度
    if width == 0 {
        let ratio = height as f64 / original_height as f64;
        let target_width = (original_width as f64 * ratio).round() as u32;
        return (target_width.max(1), height.max(1));
    }

    // 仅指定宽度时按原始宽高比推导高度
    if height == 0 {
        let ratio = width as f64 / original_width as f64;
        let target_height = (original_height as f64 * ratio).round() as u32;
        return (width.max(1), target_height.max(1));
    }

    // 同时指定宽高时取较小缩放比例，确保结果完整落在目标框内
    let width_ratio = width as f64 / original_width as f64;
    let height_ratio = height as f64 / original_height as f64;
    let ratio = width_ratio.min(height_ratio);

    let target_width = (original_width as f64 * ratio).round() as u32;
    let target_height = (original_height as f64 * ratio).round() as u32;

    (target_width.max(1), target_height.max(1))
}

/// 按指定图像格式编码图片字节
fn encode_image(image: DynamicImage, format: ImageFormat) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), format)?;
    Ok(bytes)
}

/// 查找验证码字符的 8x8 字形
fn resolve_glyph(character: char) -> Option<[u8; 8]> {
    BASIC_FONTS
        .get(character)
        .or_else(|| LATIN_FONTS.get(character))
        .or_else(|| GREEK_FONTS.get(character))
        .or_else(|| HIRAGANA_FONTS.get(character))
        .or_else(|| BLOCK_FONTS.get(character))
        .or_else(|| BOX_FONTS.get(character))
        .or_else(|| MISC_FONTS.get(character))
        .or_else(|| SGA_FONTS.get(character))
}

/// 绘制 8x8 位图字形
fn draw_glyph(image: &mut RgbaImage, glyph: [u8; 8], x: u32, y: u32, color: Rgba<u8>) {
    for (row, row_bits) in glyph.into_iter().enumerate() {
        for column in 0..8 {
            if row_bits & (1 << column) != 0 {
                fill_rect(image, x + column * 3, y + row as u32 * 4, 3, 4, color);
            }
        }
    }
}

/// 绘制纯色矩形
fn fill_rect(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32, color: Rgba<u8>) {
    for current_x in x..x.saturating_add(width) {
        for current_y in y..y.saturating_add(height) {
            if current_x < image.width() && current_y < image.height() {
                image.put_pixel(current_x, current_y, color);
            }
        }
    }
}
