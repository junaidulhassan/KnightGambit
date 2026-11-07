use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    pub struct BoardFrame {
        pub top: RefCell<Option<gtk::Widget>>,
        pub board: RefCell<Option<gtk::Widget>>,
        pub bottom: RefCell<Option<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BoardFrame {
        const NAME: &'static str = "KgBoardFrame";
        type Type = super::BoardFrame;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("boardframe");
        }
    }

    impl ObjectImpl for BoardFrame {
        fn dispose(&self) {
            for slot in [&self.top, &self.board, &self.bottom] {
                if let Some(child) = slot.take() {
                    child.unparent();
                }
            }
        }
    }

    impl WidgetImpl for BoardFrame {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let bars = self.bar_height();
            match orientation {
                gtk::Orientation::Horizontal => (240, 720, -1, -1),
                _ => (240 + bars, 720 + bars, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let top_height = self.child_height(&self.top);
            let bottom_height = self.child_height(&self.bottom);
            let gap = 6;
            let available_height = (height - top_height - bottom_height - gap * 2).max(64);
            let side = available_height.min(width).max(64);
            let x = (width - side) / 2;
            let total = top_height + gap + side + gap + bottom_height;
            let mut y = ((height - total) / 2).max(0);
            if let Some(top) = self.top.borrow().as_ref() {
                top.size_allocate(&gtk::Allocation::new(x, y, side, top_height), -1);
            }
            y += top_height + gap;
            if let Some(board) = self.board.borrow().as_ref() {
                board.size_allocate(&gtk::Allocation::new(x, y, side, side), -1);
            }
            y += side + gap;
            if let Some(bottom) = self.bottom.borrow().as_ref() {
                bottom.size_allocate(&gtk::Allocation::new(x, y, side, bottom_height), -1);
            }
        }
    }

    impl BoardFrame {
        fn child_height(&self, slot: &RefCell<Option<gtk::Widget>>) -> i32 {
            slot.borrow()
                .as_ref()
                .filter(|child| child.is_visible())
                .map(|child| child.measure(gtk::Orientation::Vertical, -1).1)
                .unwrap_or(0)
        }

        fn bar_height(&self) -> i32 {
            self.child_height(&self.top) + self.child_height(&self.bottom) + 12
        }
    }
}

glib::wrapper! {
    pub struct BoardFrame(ObjectSubclass<imp::BoardFrame>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl BoardFrame {
    pub fn new(top: &impl IsA<gtk::Widget>, board: &impl IsA<gtk::Widget>, bottom: &impl IsA<gtk::Widget>) -> Self {
        let frame: Self = glib::Object::new();
        frame.set_hexpand(true);
        frame.set_vexpand(true);
        let imp = frame.imp();
        for (slot, child) in [
            (&imp.top, top.as_ref()),
            (&imp.board, board.as_ref()),
            (&imp.bottom, bottom.as_ref()),
        ] {
            child.set_parent(&frame);
            slot.replace(Some(child.clone()));
        }
        frame
    }

    pub fn swap_bars(&self) {
        let imp = self.imp();
        let top = imp.top.take();
        let bottom = imp.bottom.take();
        imp.top.replace(bottom);
        imp.bottom.replace(top);
        self.queue_allocate();
    }
}
