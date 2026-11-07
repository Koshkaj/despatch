use std::ptr;
use std::slice;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBitmapImageRep, NSColor, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage,
    NSImageSymbolConfiguration, NSImageSymbolScale,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

/// The canvas every symbol is rasterized on.
///
/// It is sized for the largest place a symbol is drawn in the panel — about 30 points, on a 2×
/// display — so every symbol is downscaled rather than blown up. Every symbol is kept in every
/// tint the interface uses, so a larger canvas than needed costs memory for nothing.
const CANVAS_SIZE: usize = 64;
const CANVAS_SIZE_F64: f64 = 64.0;
const CANVAS_SIZE_ISIZE: isize = 64;
const CANVAS_SIZE_U32: u32 = 64;
const BYTES_PER_ROW_ISIZE: isize = 256;
const SYMBOL_INSET: f64 = 4.0;
/// Every symbol is rendered at one point size, so glyphs share the optical weight and cap height
/// SF Symbols are designed around instead of being stretched to fill the canvas individually.
const SYMBOL_POINT_SIZE: f64 = 37.5;
/// `NSFontWeightRegular`.
const SYMBOL_WEIGHT: f64 = 0.0;

/// A small, decoded SF Symbol ready for upload to the GPU renderer.
///
/// The pixels are an untinted coverage mask: every channel is white and the alpha channel carries
/// the rendered symbol. Callers tint the mask by replacing the color channels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemSymbolPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Renders an `AppKit` SF Symbol as a white RGBA coverage mask on a transparent background.
///
/// The symbol keeps its natural aspect ratio and is centered on a square canvas, so wide symbols
/// such as `ellipsis` are not stretched.
///
/// Returns `None` when called off the main thread or when the requested symbol is not available on
/// the running macOS version.
#[must_use]
pub fn render_system_symbol(name: &str, accessibility_label: &str) -> Option<SystemSymbolPixels> {
    let _main_thread = MainThreadMarker::new()?;
    let name = NSString::from_str(name);
    let accessibility_label = NSString::from_str(accessibility_label);
    let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &name,
        Some(&accessibility_label),
    )?;
    let color = NSColor::whiteColor();
    let configuration = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(
        SYMBOL_POINT_SIZE,
        SYMBOL_WEIGHT,
        NSImageSymbolScale::Medium,
    )
    .configurationByApplyingConfiguration(
        &NSImageSymbolConfiguration::configurationWithHierarchicalColor(&color),
    );
    let symbol = symbol.imageWithSymbolConfiguration(&configuration)?;

    let bitmap = create_bitmap()?;
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    context.setShouldAntialias(true);
    symbol.drawInRect(aspect_fit_rect(symbol.size()));
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();

    copy_rgba(&bitmap)
}

/// Centers `size` on the canvas at its natural size, shrinking only a symbol too wide to fit.
fn aspect_fit_rect(size: NSSize) -> NSRect {
    let available = CANVAS_SIZE_F64 - SYMBOL_INSET * 2.0;
    if size.width <= 0.0 || size.height <= 0.0 {
        return NSRect::new(
            NSPoint::new(SYMBOL_INSET, SYMBOL_INSET),
            NSSize::new(available, available),
        );
    }
    let scale = (available / size.width)
        .min(available / size.height)
        .min(1.0);
    let width = size.width * scale;
    let height = size.height * scale;
    NSRect::new(
        NSPoint::new(
            (CANVAS_SIZE_F64 - width) / 2.0,
            (CANVAS_SIZE_F64 - height) / 2.0,
        ),
        NSSize::new(width, height),
    )
}

fn create_bitmap() -> Option<Retained<NSBitmapImageRep>> {
    // SAFETY: AppKit documents a null `planes` pointer as a request for NSBitmapImageRep to
    // allocate and own its storage. The dimensions and row sizes describe one non-planar,
    // 8-bit-per-channel RGBA buffer exactly; the color-space singleton is non-null and retained by
    // AppKit. This function is reached only after obtaining MainThreadMarker, and the returned
    // retained object outlives every drawing and pointer access performed below.
    unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            CANVAS_SIZE_ISIZE,
            CANVAS_SIZE_ISIZE,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            BYTES_PER_ROW_ISIZE,
            32,
        )
    }
}

fn copy_rgba(bitmap: &NSBitmapImageRep) -> Option<SystemSymbolPixels> {
    let bytes_per_row = usize::try_from(bitmap.bytesPerRow()).ok()?;
    let byte_count = bytes_per_row.checked_mul(CANVAS_SIZE)?;
    let data = bitmap.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: `bitmapData` is non-null and points to NSBitmapImageRep-owned storage of at least
    // `bytesPerRow * height` bytes by the initializer contract. `bitmap` is retained for the
    // duration of the copy, no drawing occurs concurrently, and the slice is not kept after this
    // function returns. The selected device RGB, non-planar, four-sample layout is RGBA.
    let source = unsafe { slice::from_raw_parts(data.cast_const(), byte_count) };
    let mut rgba = Vec::with_capacity(CANVAS_SIZE * CANVAS_SIZE * 4);
    for row in source.chunks_exact(bytes_per_row) {
        // AppKit bitmap contexts are premultiplied; keep only the coverage so callers can tint the
        // mask with straight alpha instead of inheriting darkened antialiased edges.
        for pixel in row[..CANVAS_SIZE * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[u8::MAX, u8::MAX, u8::MAX, pixel[3]]);
        }
    }
    if rgba.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return None;
    }
    Some(SystemSymbolPixels {
        width: CANVAS_SIZE_U32,
        height: CANVAS_SIZE_U32,
        rgba,
    })
}
