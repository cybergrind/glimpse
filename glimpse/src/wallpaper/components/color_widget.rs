use adw::prelude::*;
use css_color::Srgb;
use relm4::gtk;
use relm4::prelude::*;

pub struct ColorWidget {
    area: gtk::DrawingArea,
}

#[derive(Debug, Clone)]
pub enum ColorWidgetInput {
    SetColor(String),
}

#[relm4::component(pub)]
impl SimpleComponent for ColorWidget {
    type Init = String;
    type Input = ColorWidgetInput;
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
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ColorWidget { area: root.clone() };
        let widgets = view_output!();
        sender.input(ColorWidgetInput::SetColor(color));
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: ColorWidgetInput, _sender: ComponentSender<Self>) {
        match msg {
            ColorWidgetInput::SetColor(color) => apply_color(&self.area, &color),
        }
    }
}

fn apply_color(root: &gtk::DrawingArea, color: &str) {
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
        root.queue_draw();
    }
}
