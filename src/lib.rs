use image::{ImageFormat, RgbaImage};
use std::io::Cursor;
use worker::*;

fn apply_mask(img: &mut RgbaImage) {
    let (w, h) = img.dimensions();
    let scale = w as f32 / 512.0;
    let strip_h = (17.0 * scale).round() as u32;
    let radius = (36.0 * scale).round() as u32;

    let safe_strip = strip_h.min(h) as usize;
    let safe_center_y = (strip_h + radius).min(h) as usize;
    let center_x = w.saturating_sub(radius) as usize;
    let radius_sq = (radius as i32) * (radius as i32);

    let raw = img.as_mut();
    let width_bytes = (w as usize) * 4;
    raw[..safe_strip * width_bytes].fill(0);

    let cy = (strip_h + radius) as i32;
    let cx = center_x as i32;

    for y in safe_strip..safe_center_y {
        let dy = y as i32 - cy;
        let dy_sq = dy * dy;
        let row_offset = y * width_bytes;
        for x in center_x..(w as usize) {
            let dx = x as i32 - cx;
            if dx * dx + dy_sq > radius_sq {
                let idx = row_offset + x * 4;
                raw[idx..idx + 4].fill(0);
            }
        }
    }
}

fn process_gif(bytes: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|e| Error::from(e.to_string()))?;

    let width = decoder.width();
    let height = decoder.height();
    let global_palette = decoder.global_palette().unwrap_or(&[]).to_vec();

    let mut encoder = gif::Encoder::new(out, width, height, &global_palette)
        .map_err(|e| Error::from(e.to_string()))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|e| Error::from(e.to_string()))?;

    let scale = width as f32 / 512.0;
    let strip_h = (17.0 * scale).round() as u32;
    let radius = (36.0 * scale).round() as u32;

    let safe_strip = strip_h.min(height as u32) as usize;
    let safe_center_y = (strip_h + radius).min(height as u32) as usize;
    let center_x = (width as u32).saturating_sub(radius) as usize;
    let radius_sq = (radius as i32) * (radius as i32);

    let cy = (strip_h + radius) as i32;
    let cx = center_x as i32;
    let w = width as usize;

    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| Error::from(e.to_string()))?
    {
        let mut frame = frame.clone();
        let trans_idx = frame.transparent.unwrap_or(0);
        frame.transparent = Some(trans_idx);

        let left = frame.left as usize;
        let top = frame.top as usize;
        let fw = frame.width as usize;
        let fh = frame.height as usize;

        let buf = frame.buffer.to_mut();

        // Calculate overlap with top strip
        if top < safe_strip {
            let strip_rows = (top + fh).min(safe_strip) - top;
            let len = strip_rows * fw;
            if len <= buf.len() {
                buf[..len].fill(trans_idx);
            }
        }

        // Calculate overlap with top-right corner mask
        let y_start = safe_strip.max(top);
        let y_end = safe_center_y.min(top + fh);

        for y in y_start..y_end {
            let dy = y as i32 - cy;
            let dy_sq = dy * dy;
            let fy = y - top;
            let row_offset = fy * fw;

            let x_start = center_x.max(left);
            let x_end = w.min(left + fw);

            for x in x_start..x_end {
                let dx = x as i32 - cx;
                if dx * dx + dy_sq > radius_sq {
                    let fx = x - left;
                    let idx = row_offset + fx;
                    if idx < buf.len() {
                        buf[idx] = trans_idx;
                    }
                }
            }
        }

        encoder
            .write_frame(&frame)
            .map_err(|e| Error::from(e.to_string()))?;
    }

    Ok(())
}

#[event(fetch)]
pub async fn main(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let image_url = match url.query_pairs().find(|(k, _)| k == "img") {
        Some((_, val)) => val.into_owned(),
        None => return Response::error("Missing 'img' query parameter", 400),
    };

    let mut fetch_res = Fetch::Url(Url::parse(&image_url)?).send().await?;
    if fetch_res.status_code() != 200 {
        return Response::error("Failed to fetch image", 502);
    }

    let bytes = fetch_res.bytes().await?;
    let format = image::guess_format(&bytes).unwrap_or(ImageFormat::Png);
    let mut out = Vec::with_capacity(bytes.len());

    if format == ImageFormat::Gif {
        process_gif(&bytes, &mut out)?;
    } else {
        let mut img = image::load_from_memory(&bytes)
            .map_err(|e| Error::from(e.to_string()))?
            .to_rgba8();
        apply_mask(&mut img);
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|e| Error::from(e.to_string()))?;
    }

    let mut res = Response::from_bytes(out)?;
    let content_type = if format == ImageFormat::Gif {
        "image/gif"
    } else {
        "image/png"
    };
    res.headers_mut().set("Content-Type", content_type)?;
    res.headers_mut().set("Cache-Control", "public, max-age=2592000")?;
    Ok(res)
}


