//! Make a PDF from images. Each image goes on its own page as a JPEG.
//!
//! The PDF writer is small and has no extra dependencies.

use std::fs::File;
use std::io::{BufWriter, Write};

use crate::image_conv::{encode_jpeg, fit, load};
use crate::{ConvertError, ConvertOptions, Progress};

/// Page size in points (1/72 inch) for an image at this DPI.
const DPI: f32 = 150.0;

pub fn images_to_pdf(
    inputs: &[String],
    output: &str,
    options: &ConvertOptions,
    progress: &Progress,
) -> Result<(), ConvertError> {
    if inputs.is_empty() {
        return Err(ConvertError::Input { msg: "no images given".into() });
    }

    // Object numbers: 1 = catalog, 2 = pages, then 3 per page (page, image, content).
    let page_count = inputs.len();
    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(2 + page_count * 3);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..page_count).map(|i| format!("{} 0 R", 3 + i * 3)).collect();
    objects.push(
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_count).into_bytes(),
    );

    for (i, path) in inputs.iter().enumerate() {
        progress.report(i as f32 / page_count as f32)?;
        let img = fit(load(path)?, options.max_dimension);
        let (w, h) = (img.width(), img.height());
        let mut jpeg = Vec::new();
        encode_jpeg(&img, options.quality, &mut jpeg)?;

        let page_w = w as f32 * 72.0 / DPI;
        let page_h = h as f32 * 72.0 / DPI;
        let page_obj = 3 + i * 3;
        let img_obj = page_obj + 1;
        let content_obj = page_obj + 2;

        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w:.2} {page_h:.2}] \
                 /Resources << /XObject << /Im0 {img_obj} 0 R >> >> /Contents {content_obj} 0 R >>"
            )
            .into_bytes(),
        );

        let mut img_body = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        img_body.extend_from_slice(&jpeg);
        img_body.extend_from_slice(b"\nendstream");
        objects.push(img_body);

        let content = format!("q {page_w:.2} 0 0 {page_h:.2} 0 0 cm /Im0 Do Q");
        objects.push(
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        );
    }

    let mut out = BufWriter::new(File::create(output)?);
    let mut offset: usize = 0;
    let write = |out: &mut BufWriter<File>, bytes: &[u8], offset: &mut usize| -> std::io::Result<()> {
        out.write_all(bytes)?;
        *offset += bytes.len();
        Ok(())
    };

    write(&mut out, b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n", &mut offset)?;
    let mut xref = Vec::with_capacity(objects.len());
    for (n, body) in objects.iter().enumerate() {
        xref.push(offset);
        write(&mut out, format!("{} 0 obj\n", n + 1).as_bytes(), &mut offset)?;
        write(&mut out, body, &mut offset)?;
        write(&mut out, b"\nendobj\n", &mut offset)?;
    }
    let xref_start = offset;
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in xref {
        tail.push_str(&format!("{off:010} 00000 n \n"));
    }
    tail.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_start}\n%%EOF\n",
        objects.len() + 1
    ));
    write(&mut out, tail.as_bytes(), &mut offset)?;
    out.flush()?;
    Ok(())
}
