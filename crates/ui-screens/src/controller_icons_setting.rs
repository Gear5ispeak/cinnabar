//! Controller button-icon selector in the JSON-UI Controller settings.

use json_ui::Catalog;
use launcher::menu::settings_options::SettingId;

/// Adds the Xbox, PlayStation 4 and PlayStation 5 icon choice below the X/Y button swap.
pub fn install(catalog: &mut Catalog) {
    super::settings_controls::install_controller_dropdown(
        catalog,
        SettingId::ControllerIcons,
        "gamepad_swap_xy",
    );
}

#[cfg(test)]
mod tests {
    use {super::*, launcher::menu::settings_options::CONTROLLER_ICON_CHOICES};

    /// Finds a caption anywhere in a resolved control.
    fn has_text(control: &json_ui::ResolvedControl, text: &str) -> bool {
        control
            .properties
            .get("text")
            .and_then(|value| value.as_str())
            == Some(text)
            || control.children.iter().any(|child| has_text(child, text))
    }

    #[test]
    fn controller_options_offer_every_icon_family() {
        let Some(carrier) = ui_engine::test_support::carrier() else {
            eprintln!(
                "skipping controller_options_offer_every_icon_family: missing UI carrier; make assets"
            );
            return;
        };
        let files = carrier.ui_files();
        let mut catalog =
            Catalog::from_files(files.iter().map(|file| (&*file.path, &*file.bytes))).unwrap();
        install(&mut catalog);
        let context = json_ui::Context::retail(false);
        let option = SettingId::ControllerIcons.definition();
        let section = json_ui::resolve(&catalog, "controls_section.controller_section", &context)
            .control
            .expect("controller options");
        assert!(has_text(&section, option.label));
        let choices = json_ui::resolve(
            &catalog,
            &format!("controls_section.{}_dropdown_content", option.name),
            &context,
        )
        .control
        .expect("controller icon choices");
        for choice in CONTROLLER_ICON_CHOICES {
            assert!(has_text(&choices, choice.label));
        }
    }
}
