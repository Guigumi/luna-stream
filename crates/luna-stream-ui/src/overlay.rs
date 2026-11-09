//! Renderizador de frame e tipos da HUD overlay in-game de alta velocidade.
//!
//! Produz frames ARGB8888 rasterizados via tiny-skia e ab_glyph com suporte a fontes
//! do sistema (fontdb), exibindo notificações visuais instantâneas sem depender do
//! toolkit de janelas principal.

use std::sync::OnceLock;

/// Tipo de evento notificado pela overlay HUD.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OverlayKind {
    /// Início do buffer de repetição instantânea.
    ReplayStarted,
    /// Interrupção do buffer de repetição instantânea.
    ReplayStopped,
    /// Salvamento com sucesso de um clipe.
    ClipSaved,
    /// Início de gravação contínua.
    RecordStarted,
    /// Término de gravação contínua.
    RecordStopped,
}

/// Dados do evento de notificação apresentado na HUD overlay.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OverlayEvent {
    pub kind: OverlayKind,
    pub title: String,
    pub subtitle: String,
}

struct FontCache {
    bold: Option<ab_glyph::FontArc>,
    regular: Option<ab_glyph::FontArc>,
}

static FONTS: OnceLock<FontCache> = OnceLock::new();

fn get_fonts() -> &'static FontCache {
    FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let query_bold = fontdb::Query {
            families: &[
                fontdb::Family::SansSerif,
                fontdb::Family::Name("Inter"),
                fontdb::Family::Name("Cantarell"),
                fontdb::Family::Name("DejaVu Sans"),
                fontdb::Family::Name("Liberation Sans"),
                fontdb::Family::Name("Adwaita Sans"),
                fontdb::Family::Name("Ubuntu"),
                fontdb::Family::Name("Roboto"),
                fontdb::Family::Name("Arial"),
            ],
            weight: fontdb::Weight::BOLD,
            ..Default::default()
        };

        let query_regular = fontdb::Query {
            families: &[
                fontdb::Family::SansSerif,
                fontdb::Family::Name("Inter"),
                fontdb::Family::Name("Cantarell"),
                fontdb::Family::Name("DejaVu Sans"),
                fontdb::Family::Name("Liberation Sans"),
                fontdb::Family::Name("Adwaita Sans"),
                fontdb::Family::Name("Ubuntu"),
                fontdb::Family::Name("Roboto"),
                fontdb::Family::Name("Arial"),
            ],
            weight: fontdb::Weight::NORMAL,
            ..Default::default()
        };

        let load_font = |query: &fontdb::Query| -> Option<ab_glyph::FontArc> {
            let id = db
                .query(query)
                .or_else(|| db.faces().next().map(|f| f.id))?;
            db.with_face_data(id, |data, face_index| {
                ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), face_index)
                    .ok()
                    .map(ab_glyph::FontArc::from)
            })
            .flatten()
        };

        let bold = load_font(&query_bold);
        let regular = load_font(&query_regular).or_else(|| bold.clone());
        let bold = bold.or_else(|| regular.clone());

        FontCache { bold, regular }
    })
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

fn blend_pixel(
    pixmap: &mut tiny_skia::Pixmap,
    x: u32,
    y: u32,
    color: tiny_skia::Color,
    coverage: f32,
) {
    if coverage <= 0.0 {
        return;
    }
    let idx = ((y * pixmap.width() + x) * 4) as usize;
    let data = pixmap.data_mut();

    let cov = (coverage.clamp(0.0, 1.0) * 255.0).round() as u32;
    if cov == 0 {
        return;
    }

    let sa = ((color.alpha() * 255.0).round() as u32 * cov) / 255;
    if sa == 0 {
        return;
    }

    let sr = ((color.red() * 255.0).round() as u32 * sa + 127) / 255;
    let sg = ((color.green() * 255.0).round() as u32 * sa + 127) / 255;
    let sb = ((color.blue() * 255.0).round() as u32 * sa + 127) / 255;

    let dst_r = data[idx] as u32;
    let dst_g = data[idx + 1] as u32;
    let dst_b = data[idx + 2] as u32;
    let dst_a = data[idx + 3] as u32;

    let inv_sa = 255 - sa;
    let out_r = sr + (dst_r * inv_sa + 127) / 255;
    let out_g = sg + (dst_g * inv_sa + 127) / 255;
    let out_b = sb + (dst_b * inv_sa + 127) / 255;
    let out_a = sa + (dst_a * inv_sa + 127) / 255;

    data[idx] = out_r.min(255) as u8;
    data[idx + 1] = out_g.min(255) as u8;
    data[idx + 2] = out_b.min(255) as u8;
    data[idx + 3] = out_a.min(255) as u8;
}

fn draw_text<F: ab_glyph::Font>(
    pixmap: &mut tiny_skia::Pixmap,
    font: &F,
    text: &str,
    scale_px: f32,
    start_x: f32,
    start_y: f32,
    color: tiny_skia::Color,
) {
    use ab_glyph::ScaleFont;
    let scaled_font = font.as_scaled(scale_px);
    let mut caret = ab_glyph::point(start_x, start_y + scaled_font.ascent());
    let mut last_glyph_id = None;

    for ch in text.chars() {
        let glyph_id = scaled_font.glyph_id(ch);
        if let Some(prev) = last_glyph_id {
            caret.x += scaled_font.kern(prev, glyph_id);
        }
        last_glyph_id = Some(glyph_id);

        let mut glyph = scaled_font.scaled_glyph(ch);
        glyph.position = caret;

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px >= 0
                    && (px as u32) < pixmap.width()
                    && py >= 0
                    && (py as u32) < pixmap.height()
                {
                    blend_pixel(pixmap, px as u32, py as u32, color, coverage);
                }
            });
        }

        caret.x += scaled_font.h_advance(glyph_id);
    }
}

/// Renderiza um quadro gráfico da overlay no formato 280x56 px ARGB8888.
///
/// Desenha o cartão arredondado com fundo `#141414`, borda `#2C2C2C`,
/// faixa semântica lateral esquerda com a cor do estado, ícone oficial centralizado
/// e tipografia com título e subtítulo.
///
/// Retorna buffer cru `Vec<u8>` de 62.720 bytes em formato ARGB8888 (ordem `[B, G, R, A]` em little-endian).
pub fn render_overlay_frame(event: &OverlayEvent, icon_png: &[u8]) -> Vec<u8> {
    const OVERLAY_WIDTH: u32 = 280;
    const OVERLAY_HEIGHT: u32 = 56;

    let Some(mut pixmap) = tiny_skia::Pixmap::new(OVERLAY_WIDTH, OVERLAY_HEIGHT) else {
        return vec![0u8; (OVERLAY_WIDTH * OVERLAY_HEIGHT * 4) as usize];
    };

    let card_path = rounded_rect(0.5, 0.5, 279.0, 55.0, 6.0).unwrap_or_else(|| {
        let r = tiny_skia::Rect::from_xywh(0.5, 0.5, 279.0, 55.0).unwrap();
        tiny_skia::PathBuilder::from_rect(r)
    });

    // 1. Fundo #141414
    let mut bg_paint = tiny_skia::Paint::default();
    bg_paint.set_color_rgba8(0x14, 0x14, 0x14, 0xFF);
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &card_path,
        &bg_paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    // 2. Borda #2C2C2C de 1px
    let mut border_paint = tiny_skia::Paint::default();
    border_paint.set_color_rgba8(0x2C, 0x2C, 0x2C, 0xFF);
    border_paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(
        &card_path,
        &border_paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );

    // 3. Barra lateral semântica esquerda de 3px, clipada ao formato arredondado
    if let Some(mut mask) = tiny_skia::Mask::new(OVERLAY_WIDTH, OVERLAY_HEIGHT) {
        mask.fill_path(
            &card_path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );

        let accent_color = match event.kind {
            OverlayKind::ClipSaved | OverlayKind::RecordStopped => {
                tiny_skia::Color::from_rgba8(0xEB, 0x41, 0x41, 0xFF)
            }
            OverlayKind::ReplayStarted | OverlayKind::RecordStarted => {
                tiny_skia::Color::from_rgba8(0x22, 0xC5, 0x5E, 0xFF)
            }
            OverlayKind::ReplayStopped => tiny_skia::Color::from_rgba8(0xA0, 0xA0, 0xA0, 0xFF),
        };

        let mut bar_paint = tiny_skia::Paint::default();
        bar_paint.set_color(accent_color);
        bar_paint.anti_alias = true;

        if let Some(bar_rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, 3.0, OVERLAY_HEIGHT as f32) {
            pixmap.fill_rect(
                bar_rect,
                &bar_paint,
                tiny_skia::Transform::identity(),
                Some(&mask),
            );
        }
    }

    // 4. Ícone oficial (28x28px em x=14, y=14)
    if let Ok(icon) = tiny_skia::Pixmap::decode_png(icon_png) {
        let iw = icon.width() as f32;
        let ih = icon.height() as f32;
        if iw > 0.0 && ih > 0.0 {
            let transform =
                tiny_skia::Transform::from_translate(14.0, 14.0).pre_scale(28.0 / iw, 28.0 / ih);
            let paint = tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            };
            pixmap.draw_pixmap(0, 0, icon.as_ref(), &paint, transform, None);
        }
    }

    // 5. Tipografia com ab_glyph (Título 13px Bold #FFFFFF, Subtítulo 11px #A0A0A0)
    let fonts = get_fonts();
    if let Some(bold_font) = &fonts.bold {
        draw_text(
            &mut pixmap,
            bold_font,
            &event.title,
            13.0,
            52.0,
            12.0,
            tiny_skia::Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF),
        );
    }
    if let Some(regular_font) = &fonts.regular {
        draw_text(
            &mut pixmap,
            regular_font,
            &event.subtitle,
            11.0,
            52.0,
            31.0,
            tiny_skia::Color::from_rgba8(0xA0, 0xA0, 0xA0, 0xFF),
        );
    }

    // 6. Conversão de RGBA pré-multiplicado para ARGB8888 (formato wl_shm em little-endian: [B, G, R, A])
    let data = pixmap.data();
    let mut argb = Vec::with_capacity(data.len());
    let (chunks, _) = data.as_chunks::<4>();
    for &[r, g, b, a] in chunks {
        argb.push(b);
        argb.push(g);
        argb.push(r);
        argb.push(a);
    }
    argb
}

/// Controlador da HUD Overlay via Wayland Layer-Shell (stub - implementação completa requer API específica).
///
/// A renderização do frame já está implementada em `render_overlay_frame`.
/// Esta função retorna um sender que pode ser usado para enviar eventos de overlay.
/// Em ambientes sem Wayland/layer-shell, opera em modo no-op.
pub fn spawn_overlay_controller(_icon_png: Vec<u8>, _overlay_enabled: bool) -> calloop::channel::Sender<OverlayCommand> {
    let (sender, _channel) = calloop::channel::channel();
    sender
}

/// Comando interno para o loop do OverlayController (stub).
#[derive(Debug, Clone)]
pub enum OverlayCommand {
    /// Mostra/atualiza a overlay com o evento dado.
    Show(OverlayEvent),
    /// Fecha a overlay imediatamente.
    Close,
    /// Encerra o controlador.
    Quit,
}
