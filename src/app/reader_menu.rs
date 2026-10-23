//! The reader's context menu (MDV-048 P6). A right press in the reader
//! reports, in order: the press itself ([`Message::PointerRightPress`]), the
//! element under it when it has its own items ([`Message::ReaderTarget`]),
//! then what the reader's text holds there ([`Message::ReaderContextMenu`]):
//! the link under the pointer and the selected text. The press changes no
//! selection.

use super::*;

impl App {
    /// Keep the innermost reader element of the current right press.
    pub(super) fn note_reader_target(&mut self, target: ReaderTarget) {
        self.reader_menu_target.get_or_insert(target);
    }

    /// Open the reader's menu for `hit` and the element noted for the same
    /// press.
    pub(super) fn open_reader_context_menu(&mut self, hit: ReaderHit) -> Task<Message> {
        let target = self.reader_menu_target.take();
        if self.file.is_none() || self.overlay != Overlay::None {
            return Task::none();
        }
        let entries = self.reader_menu_entries(&hit, target.as_ref());
        self.open_context_menu(entries, None);
        Task::none()
    }

    fn reader_menu_entries(
        &self,
        hit: &ReaderHit,
        target: Option<&ReaderTarget>,
    ) -> Vec<MenuEntry> {
        let whole_document = self.reader_select_all && self.reader_shows_document();
        let copy = if whole_document {
            MenuEntry::item("Copy", Message::CopyDocument)
        } else {
            match &hit.selection {
                Some(text) => MenuEntry::item("Copy", Message::CopyCode(text.clone())),
                None => MenuEntry::disabled("Copy"),
            }
        }
        .shortcut(Some("⌘C"));
        let select_all = if self.reader_shows_document() && !self.ast.is_empty() {
            MenuEntry::item("Select All", Message::SelectAllDocument)
        } else {
            MenuEntry::disabled("Select All")
        }
        .shortcut(Some("⌘A"));
        let selection = vec![copy, select_all];

        // The link and the element under the pointer.
        let mut here: Vec<MenuEntry> = Vec::new();
        let link = hit.link.as_deref().or(match target {
            Some(ReaderTarget::Image { link, .. }) => link.as_deref(),
            _ => None,
        });
        if let Some(link) = link {
            here.push(MenuEntry::item(
                "Open Link",
                Message::OpenLink(link.to_owned()),
            ));
            here.push(MenuEntry::item(
                "Copy Link Address",
                Message::CopyCode(link.to_owned()),
            ));
        }
        let mut element = Vec::new();
        match target {
            Some(ReaderTarget::Image { key, .. }) => {
                element.push(if self.image_viewable(key) {
                    MenuEntry::item("Open Image", Message::OpenImageZoom(key.clone()))
                } else {
                    MenuEntry::disabled("Open Image")
                });
                // Iced's clipboard holds text only: the address, not the
                // picture.
                element.push(MenuEntry::item(
                    "Copy Image Address",
                    Message::CopyCode(key.clone()),
                ));
            }
            Some(ReaderTarget::Code(code)) => {
                element.push(MenuEntry::item(
                    "Copy Code",
                    Message::CopyCode(code.clone()),
                ));
            }
            Some(ReaderTarget::Heading(id)) => {
                element.push(match self.heading_link(*id) {
                    Some(link) => MenuEntry::item("Copy Link to Heading", Message::CopyCode(link)),
                    None => MenuEntry::disabled("Copy Link to Heading"),
                });
            }
            Some(ReaderTarget::Diagram(hash)) => {
                element.push(MenuEntry::item(
                    "Copy Source",
                    Message::CopyDiagramSource(*hash),
                ));
            }
            Some(ReaderTarget::Math(source)) => {
                element.push(MenuEntry::item(
                    "Copy Source",
                    Message::CopyCode(source.clone()),
                ));
            }
            None => {}
        }
        let blank = here.is_empty() && element.is_empty();
        let mut sections = Vec::new();
        // A press inside the selection is about the selection first.
        if hit.in_selection || whole_document {
            sections.push(selection);
            sections.push(here);
            sections.push(element);
        } else {
            sections.push(here);
            sections.push(element);
            sections.push(selection);
        }
        if blank {
            sections.push(self.reader_document_entries());
            sections.push(self.reader_view_entries());
        }
        let mut entries = Vec::new();
        for section in sections.into_iter().filter(|s| !s.is_empty()) {
            if !entries.is_empty() {
                entries.push(MenuEntry::Separator);
            }
            entries.extend(section);
        }
        entries
    }

    /// Copy Path and Reveal in Finder for the open file. Their shortcuts
    /// show only while the shortcuts act on the same file.
    fn reader_document_entries(&self) -> Vec<MenuEntry> {
        let Some(file) = self.file.clone() else {
            return Vec::new();
        };
        let same = self.focused_file_path().as_ref() == Some(&file);
        let copy = match file.to_str() {
            Some(text) => MenuEntry::item("Copy Path", Message::CopyPath(text.to_owned())),
            None => MenuEntry::disabled("Copy Path"),
        };
        vec![
            copy.shortcut(same.then_some("⌘⌥C")),
            MenuEntry::item("Reveal in Finder", Message::RevealPath(file))
                .shortcut(same.then_some("⌘⌥R")),
        ]
    }

    /// The view switch: the current view is shown, greyed out.
    fn reader_view_entries(&self) -> Vec<MenuEntry> {
        [
            (ToolbarView::Read, ViewMode::Rendered, "Read", None),
            (ToolbarView::Edit, ViewMode::Raw, "Edit", Some("⌘E")),
            (
                ToolbarView::Mindmap,
                ViewMode::Mindmap,
                "Mind Map",
                Some("⌘M"),
            ),
        ]
        .into_iter()
        .map(|(view, mode, label, shortcut)| {
            if self.view_mode == mode {
                MenuEntry::disabled(label)
            } else {
                MenuEntry::item(label, Message::ToolbarView(view)).shortcut(shortcut)
            }
        })
        .collect()
    }

    /// The text of heading `id`.
    pub(super) fn heading_text(&self, id: crate::ast::BlockId) -> Option<String> {
        self.ast.iter().find_map(|(block_id, block)| match block {
            Block::Heading { inlines, .. } if *block_id == id => {
                Some(crate::ipc::sections::inline_text(inlines))
            }
            _ => None,
        })
    }

    /// `<path>#<slug>` for heading `id`: the path from the workspace root
    /// (or the file name outside a workspace), and the slug the link
    /// resolver matches.
    pub(super) fn heading_link(&self, id: crate::ast::BlockId) -> Option<String> {
        self.heading_link_for(&self.heading_text(id)?)
    }

    /// `<path>#<slug>` for a heading of the open document titled `title`.
    pub(super) fn heading_link_for(&self, title: &str) -> Option<String> {
        let slug = slugify(title);
        let file = self.file.as_deref()?;
        let relative = self
            .workspace
            .as_deref()
            .and_then(|root| file.strip_prefix(root).ok())
            .filter(|rel| !rel.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(file.file_name().unwrap_or_default()));
        let path = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        Some(format!("{path}#{slug}"))
    }

    /// Whether the viewer can show the image cached under `key`: not one
    /// that failed or is still loading.
    fn image_viewable(&self, key: &str) -> bool {
        match self.image_cache.get(key) {
            Some(ImageState::Failed | ImageState::Loading) => false,
            Some(_) => true,
            // A local image the renderer loads itself; a remote one that
            // has not arrived yet shows a placeholder.
            None => !is_remote_url(key),
        }
    }
}
