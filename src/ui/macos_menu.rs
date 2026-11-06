use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

use crate::i18n::Language;

#[derive(Clone, Copy)]
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
];

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
            (Command::New, language.menu_new_project(), "n", false),
            (Command::Open, language.menu_open_project(), "o", false),
            (Command::Save, language.menu_save_project(), "s", false),
            (Command::SaveAs, language.menu_save_project_as(), "s", true),
            (Command::Reveal, language.menu_reveal_in_finder(), "", false),
            (
                Command::Export,
                translated("Tracks exportieren …", "Export tracks …"),
                "e",
                true,
            ),
        ];
        let edit = [
            // Let the focused text field receive undo/redo keystrokes through egui.
            (Command::Undo, language.menu_undo(), "", false),
            (Command::Redo, language.menu_redo(), "", false),
        ];
        let view = [
            (
                Command::Record,
                translated("Aufnahme", "Record"),
                "1",
                false,
            ),
            (Command::Levels, translated("Pegel", "Levels"), "2", false),
            (Command::Edit, translated("Schnitt", "Edit"), "3", false),
        ];
        let mut groups = Vec::new();
        for (title, entries) in [
            (language.menu_file(), file.as_slice()),
            (language.menu_edit(), edit.as_slice()),
            (translated("Ansicht", "View"), view.as_slice()),
        ] {
            let title = NSString::from_str(title);
            let group = NSMenuItem::new(mtm);
            group.setTitle(&title);
            let submenu = NSMenu::initWithTitle(mtm.alloc(), &title);
            for &(command, title, key, shift) in entries {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        mtm.alloc(),
                        &NSString::from_str(title),
                        Some(sel!(tapeDigizerMenuAction:)),
                        &NSString::from_str(key),
                    )
                };
                if shift {
                    item.setKeyEquivalentModifierMask(
                        NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
                    );
                }
                item.setTag(command as isize);
                unsafe { item.setTarget(Some(&target)) };
                submenu.addItem(&item);
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
