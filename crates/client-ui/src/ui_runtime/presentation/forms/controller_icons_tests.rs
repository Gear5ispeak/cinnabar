use std::sync::Arc;

use launcher::menu::settings_options::{ControllerIcons, SettingId};
use launcher::menu::{MenuAction, MenuScreen, MenuView};
use player_state::PlayerState;
use ui::DpiScale;

use crate::test_support::engine_presentation;
use crate::ui_runtime::UiRuntime;

#[test]
fn controller_icon_dropdown_rows_select_their_own_choice_from_every_selection() {
    let Some(mut presentation) = engine_presentation() else {
        eprintln!("skipping controller icon dropdown: missing local UI carrier (make assets)");
        return;
    };
    let index = SettingId::ControllerIcons.index() as u16;
    let player = PlayerState::new(1);
    for selected in ControllerIcons::ALL {
        let mut view = MenuView::new(true, "Steve".into());
        view.screen = MenuScreen::Settings;
        view.settings.section = launcher::menu::settings_sections::SETTINGS_SECTIONS
            .iter()
            .find_map(|(name, index)| {
                (*name == "controller_and_switch_forced_index").then_some(*index)
            })
            .unwrap();
        view.settings.dropdown = Some(index);
        Arc::make_mut(&mut view.settings_options)
            .set(SettingId::ControllerIcons.index(), selected as i32);
        presentation.set_menu_view(Some(view));
        presentation
            .build(
                &player,
                &UiRuntime::new(1),
                0,
                [1280, 720],
                DpiScale::new(1.0).unwrap(),
            )
            .unwrap();
        for choice in ControllerIcons::ALL {
            let action = MenuAction::SettingsOption(index, choice as i32);
            let bounds = presentation
                .menu_hit_targets
                .iter()
                .find_map(|(target, bounds)| (*target == action).then_some(*bounds))
                .unwrap_or_else(|| panic!("{choice:?} is clickable while {selected:?} is saved"));
            let point = ui::UiPoint::new(
                (bounds.min().x() + bounds.max().x()) / 2.0,
                (bounds.min().y() + bounds.max().y()) / 2.0,
            )
            .unwrap();
            assert_eq!(
                presentation.hit_test_menu(point),
                Some(action),
                "{choice:?} while {selected:?} is saved"
            );
        }
    }
}
