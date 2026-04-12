use adw::prelude::*;
use css_color::Srgb;
use relm4::gtk;
use relm4::prelude::*;

pub struct ColorWidget;

#[relm4::component(pub)]
impl SimpleComponent for ColorWidget {
    type Init = String;
    type Input = ();
    type Output = ();

    view! {
        gtk::DrawingArea {
            set_hexpand: true,
            set_vexpand: true,
        }
    }

    fn init(
        color: String,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        if let Ok(Srgb {
            red,
            green,
            blue,
            alpha,
        }) = color.parse::<Srgb>()
        {
            root.set_draw_func(move |_, cr, _, _| {
                cr.set_source_rgba(red as f64, green as f64, blue as f64, alpha as f64);
                let _ = cr.paint();
            });
        }
        let model = ColorWidget;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>) {}
}
