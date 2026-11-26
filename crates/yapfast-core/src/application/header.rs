//! What a conversation's header opens: its pinned messages, a search of a
//! DM's messages or a whole server's, and the picker that adds friends to a
//! DM.

use super::*;

/// How many messages a page of search results holds, as Discord sends them.
const SEARCH_PAGE: u32 = 25;

impl Application {
    pub(super) fn show_pins(&mut self, shown: bool) -> Vec<Effect> {
        self.state.pins = None;
        let Some(room) = self
            .state
            .conversation_that(|can| can.pins)
            // A forum's messages are its posts'.
            .filter(|room| shown && !room.forum)
            .map(|room| room.id.clone())
        else {
            return vec![];
        };
        self.state.add_people = None;
        let mut effects = self.close_threads();
        self.state.pins = Some(Pins {
            room: room.clone(),
            messages: None,
        });
        effects.push(Effect::Backend(Command::Discord(DiscordCommand::Pins {
            room,
        })));
        effects
    }

    pub(super) fn pins_came(&mut self, room: String, result: Result<Vec<ChatMessage>, String>) {
        if let Some(pins) = self.state.pins.as_mut().filter(|pins| pins.room == room) {
            pins.messages = Some(result);
        }
    }

    pub(super) fn search(
        &mut self,
        query: String,
        bindings: Vec<chat::SearchUserBinding>,
    ) -> Vec<Effect> {
        let query = query.trim().to_owned();
        let (Some(place), false) = (self.state.search_place(), query.is_empty()) else {
            return self.close_search();
        };
        let place_label = self.state.search_place_label(&place);
        self.state.edit_history = None;
        self.search_request = self.search_request.wrapping_add(1);
        let generation = self.search_request;
        self.state.search = Some(Search {
            place: place.clone(),
            place_label,
            query: query.clone(),
            bindings: bindings.clone(),
            generation,
            loading: true,
            ..Search::default()
        });
        vec![Effect::Backend(Command::Discord(DiscordCommand::Search {
            place,
            query,
            bindings,
            request: generation,
            offset: 0,
        }))]
    }

    pub(super) fn search_here(&mut self) -> Vec<Effect> {
        let Some(search) = self.state.search.as_ref() else {
            return vec![];
        };
        if self.state.search_place().is_none() {
            return vec![];
        }
        let query = search.query.clone();
        let bindings = search.bindings.clone();
        self.search(query, bindings)
    }

    pub(super) fn close_search(&mut self) -> Vec<Effect> {
        let had = self.state.search.take().is_some();
        self.search_request = self.search_request.wrapping_add(1);
        self.trim_offscreen_pictures(false);
        if had {
            vec![Effect::Backend(Command::Discord(DiscordCommand::ClearSearch))]
        } else {
            vec![]
        }
    }

    pub(super) fn search_viewport(&mut self, start: usize, end: usize) -> Vec<Effect> {
        let Some(search) = self.state.search.as_mut() else {
            return vec![];
        };
        if search.results.is_empty() {
            return vec![];
        }
        let keep_start = start.saturating_sub(12);
        let keep_end = end.saturating_add(12).min(search.results.len());
        for (idx, hit) in search.results.iter_mut().enumerate() {
            if idx < keep_start || idx >= keep_end {
                let mut cleared = false;
                for att in &mut hit.message.attachments {
                    if att.preview.take().is_some() {
                        att.preview_failed = false;
                        cleared = true;
                    }
                }
                if cleared {
                    search.preview_requested.remove(&hit.message.item);
                }
            }
        }
        let req_start = start.saturating_sub(2);
        let req_end = end.saturating_add(4).min(search.results.len());
        let mut items = Vec::new();
        for hit in &search.results[req_start..req_end] {
            if search.preview_requested.contains(&hit.message.item) {
                continue;
            }
            let needs_preview = hit
                .message
                .attachments
                .iter()
                .any(|a| (a.image || a.video) && a.preview.is_none() && !a.preview_failed);
            if needs_preview {
                search.preview_requested.insert(hit.message.item.clone());
                items.push((hit.room.clone(), hit.message.item.clone()));
            }
        }
        if items.is_empty() {
            vec![]
        } else {
            vec![Effect::Backend(Command::Discord(
                DiscordCommand::SearchPreviews {
                    request: search.generation,
                    items,
                },
            ))]
        }
    }

    pub(super) fn search_preview(
        &mut self,
        request: u64,
        item: String,
        index: usize,
        preview: Option<Arc<Picture>>,
    ) {
        let Some(search) = self
            .state
            .search
            .as_mut()
            .filter(|search| search.generation == request)
        else {
            return;
        };
        if let Some(hit) = search
            .results
            .iter_mut()
            .find(|hit| hit.message.item == item)
            && let Some(attachment) = hit.message.attachments.get_mut(index)
        {
            match preview {
                Some(picture) => {
                    attachment.preview = Some(picture);
                    attachment.preview_failed = false;
                }
                None => {
                    attachment.preview_failed = true;
                }
            }
        }
    }

    pub(super) fn search_start_at(&mut self, position: String) -> Vec<Effect> {
        let Some(search) = self.state.search.as_mut() else {
            return vec![];
        };
        search.jump_revision = search.jump_revision.wrapping_add(1);
        let position = position.trim();
        search.jump_to = position
            .parse::<u32>()
            .ok()
            .filter(|&number| number > 0 && number <= search.total)
            .filter(|_| position.bytes().all(|byte| byte.is_ascii_digit()));
        search.jump_hint = if search.jump_to.is_none() {
            format!("Enter a result number from 1 to {}", search.total)
        } else {
            String::new()
        };
        self.more_results_for_jump()
    }

    fn more_results_for_jump(&mut self) -> Vec<Effect> {
        if self.state.search.as_ref().is_some_and(|search| {
            search.jump_to.is_some_and(|number| number as usize > search.results.len())
        }) {
            return self.more_results();
        }
        vec![]
    }

    pub(super) fn more_results(&mut self) -> Vec<Effect> {
        let Some(search) = self
            .state
            .search
            .as_mut()
            .filter(|search| !search.loading && search.more())
        else {
            return vec![];
        };
        search.loading = true;
        search.error = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::Search {
            place: search.place.clone(),
            query: search.query.clone(),
            bindings: search.bindings.clone(),
            request: search.generation,
            offset: search.next,
        }))]
    }

    /// Takes a page of results for the search on screen, if it's the page
    /// it asked for.
    pub(super) fn found(
        &mut self,
        request: u64,
        offset: u32,
        result: Result<(u32, Vec<chat::SearchHit>), String>,
    ) -> Vec<Effect> {
        let Some(search) = self.state.search.as_mut().filter(|search| {
            search.loading && self.search_request == request && search.next == offset
        }) else {
            return vec![];
        };
        search.loading = false;
        match result {
            Ok((total, page)) => {
                // Messages sent since the last page move the rest along.
                for hit in page {
                    if !search
                        .results
                        .iter()
                        .any(|known| known.message.item == hit.message.item)
                    {
                        search.results.push(hit);
                    }
                }
                search.total = total;
                search.next = offset + SEARCH_PAGE;
            }
            Err(error) => {
                search.jump_to = None;
                search.error = Some(error);
                return vec![];
            }
        }
        if search.jump_to.is_some_and(|number| {
            number > search.total || (!search.more() && number as usize > search.results.len())
        }) {
            search.jump_to = None;
            search.jump_hint = "That result is unavailable. Try another number.".into();
        }
        self.more_results_for_jump()
    }

    pub(super) fn open_add_people(&mut self) -> Vec<Effect> {
        let room = self
            .state
            .dm_that(|can| can.group_dms)
            .filter(|_| self.state.discord_online)
            .map(|room| room.id.clone());
        if let Some(room) = room {
            self.state.pins = None;
            self.state.add_people = Some(AddPeople {
                room,
                ..AddPeople::default()
            });
        }
        vec![]
    }

    pub(super) fn choose_person(&mut self, user: String) -> Vec<Effect> {
        let room = self.state.group_room();
        let Some(picker) = self
            .state
            .add_people
            .as_mut()
            .filter(|picker| !picker.adding)
        else {
            return vec![];
        };
        if let Some(index) = picker.chosen.iter().position(|chosen| chosen == &user) {
            picker.chosen.remove(index);
        } else if picker.chosen.len() < room {
            picker.chosen.push(user);
        }
        picker.error = None;
        vec![]
    }

    /// Adds those chosen to the group DM, or with a one-to-one DM, makes a
    /// group of them and its other person, as Discord's client does; the
    /// DM itself stays as it was.
    pub(super) fn add_people(&mut self) -> Vec<Effect> {
        let person = self.state.one_to_one().and_then(|room| room.person.clone());
        let Some(picker) = self
            .state
            .add_people
            .as_mut()
            .filter(|picker| !picker.adding && !picker.chosen.is_empty())
        else {
            return vec![];
        };
        picker.adding = true;
        picker.error = None;
        let command = match person {
            Some(person) => DiscordCommand::AddToGroup {
                room: None,
                users: std::iter::once(person)
                    .chain(picker.chosen.iter().cloned())
                    .collect(),
            },
            None => DiscordCommand::AddToGroup {
                room: Some(picker.room.clone()),
                users: picker.chosen.clone(),
            },
        };
        vec![Effect::Backend(Command::Discord(command))]
    }

    /// A group made opens once the DM list has it.
    pub(super) fn group_changed(&mut self, result: Result<String, String>) {
        match result {
            Ok(room) => {
                if self.state.add_people.take().is_some()
                    && self.state.selected.as_ref() != Some(&room)
                {
                    self.pending_open = Some(room);
                }
            }
            Err(error) => match &mut self.state.add_people {
                Some(picker) => {
                    picker.adding = false;
                    picker.error = Some(error);
                }
                None => self.state.notice = Some(error),
            },
        }
    }
}
