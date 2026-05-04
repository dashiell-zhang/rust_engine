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
    let cropped = crop_imm(&image, offset_x, offset_y, width, height).to_image();
    encode_png(DynamicImage::ImageRgba8(cropped))
}

/// 获取图像数字验证码
pub fn get_verify_code(text: &str) -> Result<Vec<u8>, image::ImageError> {
    let digits = validate_verify_code_text(text)?;
    let width = 128;
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

    for (x, ch) in [10, 38, 66, 94].into_iter().zip(digits.chars()) {
        draw_digit(
            &mut image,
            ch,
            x,
            8,
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

/// 校验图片验证码文本
fn validate_verify_code_text(text: &str) -> Result<&str, image::ImageError> {
    if text.is_empty() || text.len() > 4 || !text.chars().all(|item| item.is_ascii_digit()) {
        return Err(image::ImageError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "验证码必须是 1 到 4 位数字",
        )));
    }

    Ok(text)
}

/// 将图片字节按指定宽高缩放并保持原始格式
pub fn resize_image(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, image::ImageError> {
    let format = image::guess_format(data)?;

    // 未指定缩放尺寸时直接返回原始字节
    if width == 0 && height == 0 {
        return Ok(data.to_vec());
    }

    let image = image::load_from_memory(data)?;
    let (target_width, target_height) = calculate_resize_dimensions(&image, width, height);

    // 与 NetEngine 对齐：目标尺寸不小于原图时直接返回原图，避免无意义放大。
    if target_width >= image.width() || target_height >= image.height() {
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

/// 使用七段码绘制数字字符
fn draw_digit(image: &mut RgbaImage, ch: char, x: u32, y: u32, color: Rgba<u8>) {
    let segments = match ch {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => [false, false, false, false, false, false, false],
    };

    let thickness = 3;
    let length = 14;

    if segments[0] {
        fill_rect(image, x + thickness, y, length, thickness, color);
    }
    if segments[1] {
        fill_rect(
            image,
            x + thickness + length,
            y + thickness,
            thickness,
            length,
            color,
        );
    }
    if segments[2] {
        fill_rect(
            image,
            x + thickness + length,
            y + thickness + length,
            thickness,
            length,
            color,
        );
    }
    if segments[3] {
        fill_rect(
            image,
            x + thickness,
            y + thickness * 2 + length * 2,
            length,
            thickness,
            color,
        );
    }
    if segments[4] {
        fill_rect(image, x, y + thickness + length, thickness, length, color);
    }
    if segments[5] {
        fill_rect(image, x, y + thickness, thickness, length, color);
    }
    if segments[6] {
        fill_rect(
            image,
            x + thickness,
            y + thickness + length,
            length,
            thickness,
            color,
        );
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
