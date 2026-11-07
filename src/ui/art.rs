use std::cell::RefCell;
use std::collections::HashMap;

use gtk::prelude::*;
use gtk::{gdk, glib};
use shakmaty::{Color, Role};

use crate::assets::{PieceStyle, render_piece_rgba, render_svg_rgba};

pub const ROLES: [Role; 6] = [
    Role::King,
    Role::Queen,
    Role::Rook,
    Role::Bishop,
    Role::Knight,
    Role::Pawn,
];

pub fn texture_from_rgba(data: Vec<u8>, width: u32, height: u32) -> gdk::Texture {
    let bytes = glib::Bytes::from_owned(data);
    gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &bytes,
        width as usize * 4,
    )
    .upcast()
}

type PieceKey = (PieceStyle, Role, Color, u32, u64);

#[derive(Default)]
pub struct PieceArt {
    cache: RefCell<HashMap<PieceKey, gdk::Texture>>,
}

impl PieceArt {
    pub fn texture(&self, style: PieceStyle, role: Role, color: Color, pixels: u32) -> Option<gdk::Texture> {
        let pixels = pixels.clamp(8, 1024);
        let key = (style, role, color, pixels, crate::assets::custom_revision());
        if let Some(texture) = self.cache.borrow().get(&key) {
            return Some(texture.clone());
        }
        let data = render_piece_rgba(role, color, style, pixels)?;
        let texture = texture_from_rgba(data, pixels, pixels);
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 96 {
            cache.clear();
        }
        cache.insert(key, texture.clone());
        Some(texture)
    }
}

thread_local! {
    static SHARED_ART: PieceArt = PieceArt::default();
}

pub fn small_piece_texture(style: PieceStyle, role: Role, color: Color, pixels: u32) -> Option<gdk::Texture> {
    SHARED_ART.with(|art| art.texture(style, role, color, pixels))
}

pub fn svg_texture(svg: &str, pixels: u32) -> Option<gdk::Texture> {
    render_svg_rgba(svg, pixels).map(|data| texture_from_rgba(data, pixels, pixels))
}

pub fn piece_picture(style: PieceStyle, role: Role, color: Color, logical: i32, scale: i32) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(logical, logical);
    if let Some(texture) = small_piece_texture(style, role, color, (logical * scale.max(1)) as u32) {
        picture.set_paintable(Some(&texture));
    }
    picture
}

thread_local! {
    static REVIEW_ICONS: RefCell<HashMap<(crate::review::MoveClass, u32), gdk::Texture>> = RefCell::new(HashMap::new());
}

pub fn review_icon_texture(class: crate::review::MoveClass, pixels: u32) -> Option<gdk::Texture> {
    let pixels = pixels.clamp(8, 512);
    REVIEW_ICONS.with(|icons| {
        if let Some(texture) = icons.borrow().get(&(class, pixels)) {
            return Some(texture.clone());
        }
        let texture = svg_texture(&crate::review::review_icon_svg(class), pixels)?;
        icons.borrow_mut().insert((class, pixels), texture.clone());
        Some(texture)
    })
}

pub fn review_icon_picture(class: crate::review::MoveClass, logical: i32, scale: i32) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(logical, logical);
    picture.set_valign(gtk::Align::Center);
    picture.set_tooltip_text(Some(class.label()));
    if let Some(texture) = review_icon_texture(class, (logical * scale.max(1)) as u32) {
        picture.set_paintable(Some(&texture));
    }
    picture
}
