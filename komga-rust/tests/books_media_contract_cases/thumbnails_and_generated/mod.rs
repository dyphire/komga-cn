use super::*;

async fn seed_book_thumbnail_bytes(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    bytes: &[u8],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(bytes)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("book thumbnail row should be updated");
    pool.close().await;
}

fn distinct_png_bytes(width: u32, height: u32, red: u8, green: u8, blue: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([red, green, blue, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("distinct png fixture should encode");
    output.into_inner()
}

fn distinct_jpeg_bytes(width: u32, height: u32, red: u8, green: u8, blue: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([red, green, blue, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .expect("distinct jpeg fixture should encode");
    output.into_inner()
}

mod book_media_asset_routes;
mod book_thumbnail_delete_and_listing;
mod book_thumbnail_upload;
mod generated_thumbnail_persistence;
mod opds_thumbnail_routes;
