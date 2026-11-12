use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::i18n::Language;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
pub enum Command {
    New = 1,
    Open,
    Save,
    SaveAs,
    Reveal,
    Undo,
    Redo,
    Record,
    Levels,
    Edit,
    Export,
    Quit,
    ToggleFilterStudio,
    ToggleScope,
    ToggleTracks,
    ToggleLiveFilter,
    LearnNoise,
    BakeFilterTrack,
    BakeFilterAll,
    FilterAllTracks,
    FilterNoTracks,
    ToggleSelectedTrackFilter,
    ToggleSubsonicFilter,
}

struct MenuState {
    ctx: egui::Context,
    commands: RefCell<Vec<Command>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuState]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(tapeDigizerMenuAction:))]
        fn invoke(&self, sender: &NSMenuItem) {
            let Some(command) = COMMANDS.iter().find(|&&command| command as isize == sender.tag()) else {
                return;
            };
            self.ivars().commands.borrow_mut().push(*command);
            self.ivars().ctx.request_repaint();
        }
    }
);

const COMMANDS: &[Command] = &[
    Command::New,
    Command::Open,
    Command::Save,
    Command::SaveAs,
    Command::Reveal,
    Command::Undo,
    Command::Redo,
    Command::Record,
    Command::Levels,
    Command::Edit,
    Command::Export,
    Command::Quit,
    Command::ToggleFilterStudio,
    Command::ToggleScope,
    Command::ToggleTracks,
    Command::ToggleLiveFilter,
    Command::LearnNoise,
    Command::BakeFilterTrack,
    Command::BakeFilterAll,
    Command::FilterAllTracks,
    Command::FilterNoTracks,
    Command::ToggleSelectedTrackFilter,
    Command::ToggleSubsonicFilter,
];

pub enum MenuItemDef<'a> {
    Item {
        command: Command,
        title: &'a str,
        key: &'a str,
        shift: bool,
    },
    Separator,
}

pub struct MacMenu {
    target: Retained<MenuTarget>,
    bar: Retained<NSMenu>,
    groups: Vec<Retained<NSMenuItem>>,
    pub language: Language,
    quit_item: Option<(Retained<NSMenuItem>, isize)>,
}

impl MacMenu {
    pub fn install(ctx: &egui::Context, language: Language) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let bar = NSApplication::sharedApplication(mtm).mainMenu()?;
        let target = mtm.alloc::<MenuTarget>().set_ivars(MenuState {
            ctx: ctx.clone(),
            commands: RefCell::new(Vec::new()),
        });
        // AppKit retains the menu items. Their weak action target is owned by MacMenu.
        let target: Retained<MenuTarget> = unsafe { msg_send![super(target), init] };
        let translated = |de, en| if language == Language::De { de } else { en };
        let file = [
            MenuItemDef::Item { command: Command::New, title: language.menu_new_project(), key: "n", shift: false },
            MenuItemDef::Item { command: Command::Open, title: language.menu_open_project(), key: "o", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::Save, title: language.menu_save_project(), key: "s", shift: false },
            MenuItemDef::Item { command: Command::SaveAs, title: language.menu_save_project_as(), key: "s", shift: true },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::Reveal, title: language.menu_reveal_in_finder(), key: "", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item {
                command: Command::Export,
                title: translated("Tracks exportieren …", "Export tracks …"),
                key: "e",
                shift: true,
            },
        ];
        let edit = [
            // Let the focused text field receive undo/redo keystrokes through egui.
            MenuItemDef::Item { command: Command::Undo, title: language.menu_undo(), key: "", shift: false },
            MenuItemDef::Item { command: Command::Redo, title: language.menu_redo(), key: "", shift: false },
        ];
        let view = [
            MenuItemDef::Item { command: Command::Record, title: translated("Aufnahme", "Record"), key: "1", shift: false },
            MenuItemDef::Item { command: Command::Levels, title: translated("Pegel", "Levels"), key: "2", shift: false },
            MenuItemDef::Item { command: Command::Edit, title: translated("Schnitt", "Edit"), key: "3", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::ToggleFilterStudio, title: language.menu_toggle_filter_studio(), key: "f", shift: false },
            MenuItemDef::Item { command: Command::ToggleScope, title: language.menu_toggle_scope(), key: "l", shift: false },
            MenuItemDef::Item { command: Command::ToggleTracks, title: language.menu_toggle_tracks(), key: "t", shift: false },
        ];
        let filter = [
            MenuItemDef::Item { command: Command::ToggleFilterStudio, title: language.menu_toggle_filter_studio(), key: "f", shift: false },
            MenuItemDef::Item { command: Command::ToggleLiveFilter, title: language.menu_toggle_live_filter(), key: "", shift: false },
            MenuItemDef::Item { command: Command::LearnNoise, title: language.menu_learn_noise(), key: "", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::BakeFilterTrack, title: language.menu_bake_selected_track(), key: "", shift: false },
            MenuItemDef::Item { command: Command::BakeFilterAll, title: language.menu_bake_all(), key: "", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::FilterAllTracks, title: language.menu_filter_all_tracks(), key: "", shift: false },
            MenuItemDef::Item { command: Command::FilterNoTracks, title: language.menu_filter_no_tracks(), key: "", shift: false },
            MenuItemDef::Item { command: Command::ToggleSelectedTrackFilter, title: language.menu_toggle_track_filter(), key: "", shift: false },
            MenuItemDef::Separator,
            MenuItemDef::Item { command: Command::ToggleSubsonicFilter, title: language.menu_toggle_subsonic(), key: "", shift: false },
        ];
        let mut groups = Vec::new();
        for (title, entries) in [
            (language.menu_file(), file.as_slice()),
            (language.menu_edit(), edit.as_slice()),
            (language.menu_view(), view.as_slice()),
            (language.menu_filter(), filter.as_slice()),
        ] {
            let title_ns = NSString::from_str(title);
            let group = NSMenuItem::new(mtm);
            group.setTitle(&title_ns);
            let submenu = NSMenu::initWithTitle(mtm.alloc(), &title_ns);
            for entry in entries {
                match entry {
                    MenuItemDef::Item { command, title, key, shift } => {
                        let item = unsafe {
                            NSMenuItem::initWithTitle_action_keyEquivalent(
                                mtm.alloc(),
                                &NSString::from_str(title),
                                Some(sel!(tapeDigizerMenuAction:)),
                                &NSString::from_str(key),
                            )
                        };
                        if *shift {
                            item.setKeyEquivalentModifierMask(
                                NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
                            );
                        }
                        item.setTag(*command as isize);
                        unsafe { item.setTarget(Some(&target)) };
                        submenu.addItem(&item);
                    }
                    MenuItemDef::Separator => {
                        let sep = NSMenuItem::separatorItem(mtm);
                        submenu.addItem(&sep);
                    }
                }
            }
            group.setSubmenu(Some(&submenu));
            bar.addItem(&group);
            groups.push(group);
        }
        let quit_item = bar
            .itemAtIndex(0)
            .and_then(|item| item.submenu())
            .and_then(|app| {
                (0..app.numberOfItems())
                    .filter_map(|index| app.itemAtIndex(index))
                    .find(|item| item.action() == Some(sel!(terminate:)))
            })
            .map(|item| {
                let old_tag = item.tag();
                item.setTag(Command::Quit as isize);
                unsafe {
                    item.setTarget(Some(&target));
                    item.setAction(Some(sel!(tapeDigizerMenuAction:)));
                }
                (item, old_tag)
            });
        Some(Self {
            target,
            bar,
            groups,
            language,
            quit_item,
        })
    }

    pub fn take_commands(&self) -> Vec<Command> {
        self.target.ivars().commands.take()
    }
}

impl Drop for MacMenu {
    fn drop(&mut self) {
        if let Some((item, tag)) = &self.quit_item {
            item.setTag(*tag);
            unsafe {
                item.setTarget(None);
                item.setAction(Some(sel!(terminate:)));
            }
        }
        for group in &self.groups {
            self.bar.removeItem(group);
        }
    }
}
