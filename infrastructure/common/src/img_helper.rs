mod qr_mask;

use qr_mask::{QrMatrix, select_mask};
use qrcode::{EcLevel, Version, bits::Bits, types::QrError};
use skia_safe::{
    AlphaType, BlendMode, Codec, Color, ColorType, Data, EncodedImageFormat, Font, FontMgr,
    FontStyle, Image, ImageInfo, Paint, Pixmap, Rect, SamplingOptions, canvas::SrcRectConstraint,
    codec, images, surfaces,
};
use std::borrow::Cow;
use std::path::Path;

/// 生成二维码
pub fn get_qr_code(text: &str) -> Result<Vec<u8>, image::ImageError> {
    let code = encode_qr_code(text)
        .map_err(|error| image::ImageError::IoError(std::io::Error::other(error.to_string())))?;
    let info = ImageInfo::new_n32_premul((500, 500), None);
    let mut surface = surfaces::raster(&info, None, None)
        .ok_or_else(|| invalid_image_input("无法创建二维码画布"))?;
    let canvas = surface.canvas();
    canvas.clear(Color::WHITE);
    let mut paint = Paint::default();
    paint.set_color(Color::BLACK);
    // 默认四模块留白；按源渲染器合并连续黑色模块，以单精度矩形坐标绘制。
    let cell = 500.0 / (code.width() + 8) as f32;
    for y in 0..code.width() {
        let mut x = 0;
        while x < code.width() {
            if !code.is_dark(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < code.width() && code.is_dark(x, y) {
                x += 1;
            }
            let top = (y + 4) as f32 * cell;
            canvas.draw_rect(
                Rect::new(
                    (start + 4) as f32 * cell,
                    top,
                    (x + 4) as f32 * cell,
                    top + cell,
                ),
                &paint,
            );
        }
    }
    encode_skia_png(&surface.image_snapshot())
}

/// 源默认采用单一编码段，不使用库的混合段优化或 Shift-JIS 自动识别。
fn encode_qr_code(text: &str) -> Result<QrMatrix, QrError> {
    let numeric = !text.is_empty() && text.bytes().all(|value| value.is_ascii_digit());
    let alphanumeric = !text.is_empty()
        && text.bytes().all(|value| {
            value.is_ascii_digit() || value.is_ascii_uppercase() || b" $%*+-./:".contains(&value)
        });
    let (data, eci): (Cow<'_, [u8]>, Option<u32>) = if text.is_ascii() {
        (Cow::Borrowed(text.as_bytes()), None)
    } else if text.chars().all(|value| u32::from(value) <= 255) {
        (
            Cow::Owned(text.chars().map(|value| value as u8).collect()),
            Some(3),
        )
    } else {
        (Cow::Borrowed(text.as_bytes()), Some(26))
    };
    // 标准二维码 L 级最多容纳 7089 个数字；其他模式容量更小。
    if data.len() > 7089 {
        return Err(QrError::DataTooLong);
    }
    for version in 1..=40 {
        let mut bits = Bits::new(Version::Normal(version));
        if let Some(eci) = eci {
            bits.push_eci_designator(eci)?;
        }
        let encoded = if numeric {
            bits.push_numeric_data(&data)
        } else if alphanumeric {
            bits.push_alphanumeric_data(&data)
        } else {
            // 空字符串也明确写入长度为零的 Byte 段。
            bits.push_byte_data(&data)
        };
        match encoded.and_then(|()| bits.push_terminator(EcLevel::L)) {
            Ok(()) => return select_mask(bits),
            Err(QrError::DataTooLong) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(QrError::DataTooLong)
}

/// 从图片截取部分区域
pub fn screenshot(
    from_image_path: impl AsRef<Path>,
    offset_x: i32,
    offset_y: i32,
    width: i32,
    height: i32,
) -> Result<Vec<u8>, image::ImageError> {
    let path = from_image_path.as_ref();
    if !path.is_file() {
        return Err(image::ImageError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "图片文件不存在",
        )));
    }
    let image = decode_image(&std::fs::read(path)?)?;
    if offset_x < 0 || offset_y < 0 {
        return Err(invalid_image_input("裁剪偏移量不能小于0"));
    }
    if width <= 0 || height <= 0 {
        return Err(invalid_image_input("裁剪宽高必须大于0"));
    }
    let end_x = i64::from(offset_x) + i64::from(width);
    let end_y = i64::from(offset_y) + i64::from(height);
    if end_x > i64::from(image.width()) || end_y > i64::from(image.height()) {
        return Err(invalid_image_input("裁剪区域超出源图片范围"));
    }
    let info = ImageInfo::new_n32_premul((width, height), None);
    let mut surface = surfaces::raster(&info, None, None)
        .ok_or_else(|| invalid_image_input("无法创建裁剪画布"))?;
    let source = Rect::new(offset_x as f32, offset_y as f32, end_x as f32, end_y as f32);
    let destination = Rect::from_wh(width as f32, height as f32);
    surface.canvas().draw_image_rect_with_sampling_options(
        &image,
        Some((&source, SrcRectConstraint::Fast)),
        destination,
        SamplingOptions::default(),
        &Paint::default(),
    );
    encode_skia_png(&surface.image_snapshot())
}

/// 获取图像验证码
pub fn get_verify_code(text: &str) -> Result<Vec<u8>, image::ImageError> {
    if text.trim().is_empty() {
        return Err(invalid_image_input("验证码文本不能为空"));
    }
    let characters = text.encode_utf16().collect::<Vec<_>>();
    if characters.len() > 8 {
        return Err(invalid_image_input("验证码文本不能超过8个字符"));
    }
    let width = 128.max(30 + characters.len() as i32 * 32);
    let height = 45;
    let info = ImageInfo::new(
        (width, height),
        ColorType::BGRA8888,
        AlphaType::Premul,
        None,
    );
    let mut surface = surfaces::raster(&info, None, None)
        .ok_or_else(|| invalid_image_input("无法创建验证码画布"))?;
    let canvas = surface.canvas();
    canvas.clear(Color::WHITE);
    let mut paint = Paint::default();
    // 源循环以整数索引与浮点上限比较，非整数上限也会执行最后一次。
    for _ in 0..(f64::from(width * height) * 0.015).ceil() as usize {
        paint.set_color(random_verify_color());
        canvas.draw_line(
            (
                rand::random_range(0..width) as f32,
                rand::random_range(0..height) as f32,
            ),
            (
                rand::random_range(0..width) as f32,
                rand::random_range(0..height) as f32,
            ),
            &paint,
        );
    }

    let mut font = FontMgr::new()
        .legacy_make_typeface(None, FontStyle::normal())
        .map(|typeface| Font::new(typeface, height as f32))
        .unwrap_or_default();
    font.set_size(height as f32);
    let colors = [
        0xffff0000, 0xff00ff00, 0xffff00ff, 0xffffff00, 0xffff4500, 0xffff1493, 0xff1e90ff,
    ];
    let mut x = 15.0;
    let y = height as f32 * 0.86;
    for character in characters {
        paint.set_color(Color::new(colors[rand::random_range(0..colors.len())]));
        // 源逐个 UTF-16 char 绘制；代理项不能独立测量，不将缺字静默当作成功。
        let character = char::from_u32(u32::from(character))
            .ok_or_else(|| invalid_image_input("验证码包含不能独立绘制的 UTF-16 字符"))?;
        let mut utf8 = [0; 4];
        let current = character.encode_utf8(&mut utf8);
        let (advance, _) = font.measure_str(&*current, Some(&paint));
        canvas.draw_str(&*current, (x, y), &font, &paint);
        x += advance;
    }

    // SetPixel 是覆盖像素；不能用默认 SrcOver 把半透明噪点混合到已有文字上。
    paint.set_blend_mode(BlendMode::Src);
    for _ in 0..(f64::from(width * height) * 0.6).ceil() as usize {
        let x = rand::random_range(0..width) as f32;
        let y = rand::random_range(0..height) as f32;
        paint.set_color(random_verify_color());
        canvas.draw_rect(Rect::from_xywh(x, y, 1.0, 1.0), &paint);
    }
    encode_skia_png(&surface.image_snapshot())
}

/// Random.Next(Int32.MaxValue) 的 ARGB 范围，保留半透明和透明噪点。
fn random_verify_color() -> Color {
    Color::new(rand::random_range(0..i32::MAX) as u32)
}

/// 按文件接口约定使用最近邻缩放为 PNG；任一目标边超过原图则返回原图标记。
pub fn resize_image(
    data: &[u8],
    mut width: i32,
    mut height: i32,
) -> Result<Option<Vec<u8>>, image::ImageError> {
    if width == 0 && height == 0 {
        return Ok(None);
    }
    let image = decode_image(data)?;
    if width > image.width() || height > image.height() {
        return Ok(None);
    }
    // 两个条件按源接口顺序独立判断；第一步截断为零会影响第二步。
    if width != 0 && height == 0 {
        let ratio = width as f32 / image.width() as f32;
        width = (image.width() as f32 * ratio) as i32;
        height = (image.height() as f32 * ratio) as i32;
    }
    if width == 0 && height != 0 {
        let ratio = height as f32 / image.height() as f32;
        width = (image.width() as f32 * ratio) as i32;
        height = (image.height() as f32 * ratio) as i32;
    }
    if width <= 0 || height <= 0 {
        return Err(invalid_image_input("图片缩放尺寸必须大于 0"));
    }
    let info = ImageInfo::new_n32_premul((width, height), None);
    let mut pixels = allocate_pixels(&info)?;
    {
        let source = image
            .peek_pixels()
            .ok_or_else(|| invalid_image_input("无法读取图片像素"))?;
        let mut destination = Pixmap::new(&info, &mut pixels, info.min_row_bytes())
            .ok_or_else(|| invalid_image_input("无法创建缩放像素缓冲"))?;
        if !source.scale_pixels(&mut destination, SamplingOptions::default()) {
            return Err(invalid_image_input("图片缩放失败"));
        }
    }
    let resized =
        images::raster_from_data(&info, Data::new_with_owner(pixels), info.min_row_bytes())
            .ok_or_else(|| invalid_image_input("无法创建缩放图片"))?;
    encode_skia_png(&resized).map(Some)
}

/// 与 SKBitmap.Decode 一样保留解码颜色信息，将非预乘透明度转为预乘；不自动应用 EXIF 旋转。
fn decode_image(data: &[u8]) -> Result<Image, image::ImageError> {
    let mut decoder = Codec::from_data(Data::new_copy(data))
        .ok_or_else(|| invalid_image_input("图片文件无法解码"))?;
    let mut info = decoder.info();
    if info.alpha_type() == AlphaType::Unpremul {
        info = info.with_alpha_type(AlphaType::Premul);
    }
    let mut pixels = allocate_pixels(&info)?;
    let result = decoder.get_pixels_with_options(&info, &mut pixels, info.min_row_bytes(), None);
    // 源实现保留不完整输入已解出的像素，不把所有非 Success 结果一律拒绝。
    if !matches!(
        result,
        codec::Result::Success | codec::Result::IncompleteInput
    ) {
        return Err(invalid_image_input("图片文件无法解码"));
    }
    images::raster_from_data(&info, Data::new_with_owner(pixels), info.min_row_bytes())
        .ok_or_else(|| invalid_image_input("无法创建解码图片"))
}

fn allocate_pixels(info: &ImageInfo) -> Result<Vec<u8>, image::ImageError> {
    let size = info.compute_min_byte_size();
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size)
        .map_err(|_| invalid_image_input("无法分配图片像素缓冲"))?;
    pixels.resize(size, 0);
    Ok(pixels)
}

fn encode_skia_png(image: &Image) -> Result<Vec<u8>, image::ImageError> {
    image
        .encode(None, EncodedImageFormat::PNG, 100)
        .map(|data| data.as_bytes().to_vec())
        .ok_or_else(|| invalid_image_input("无法编码 PNG 图片"))
}

/// 构造图片输入错误
fn invalid_image_input(message: &str) -> image::ImageError {
    image::ImageError::IoError(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}
