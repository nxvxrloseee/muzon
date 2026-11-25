//! Queue occurrence actions and playback commands sent to the controller.
use super::*;

/// Queue rows address queue occurrences, including repeated copies of a track.
pub fn displayed_queue_index(snapshot: &Snapshot, row: usize) -> Option<usize> {
    snapshot
        .display
        .get(row)
        .copied()
        .filter(|&i| i < snapshot.queue.len())
}

impl RealApp {
    pub(super) fn update_queue(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueueDragStart(from) => {
                if from < self.snapshot.display.len() {
                    self.queue_drag = Some(QueueDrag {
                        from,
                        to: from,
                        baseline: self.snapshot.display.clone(),
                    });
                }
            }
            Message::QueueDragHover(to) => {
                if to < self.snapshot.display.len() {
                    if let Some(drag) = &mut self.queue_drag {
                        drag.to = to;
                    }
                }
            }
            Message::QueueDragRelease => {
                if let Some(drag) = self.queue_drag.take() {
                    if Arc::ptr_eq(&drag.baseline, &self.snapshot.display) && drag.from != drag.to {
                        return self.update(Message::QueueMove(drag.from, drag.to));
                    }
                }
            }
            Message::QueueScrolled(y) => self.queue_scroll = y,
            Message::SeekChanged(value) => self.seek_preview = Some(value),
            other => {
                if let Some(core) = &self.controller {
                    match other {
                        Message::TrackRowClicked(row) => {
                            if let Some(id) = filtered_track_id(&self.snapshot, &self.filtered, row)
                            {
                                core.set_queue(
                                    self.filtered
                                        .iter()
                                        .filter_map(|&i| self.snapshot.library.get(i).map(|t| t.id))
                                        .collect(),
                                    id,
                                );
                            }
                        }
                        Message::EnqueueRow(row) => {
                            if let Some(id) = filtered_track_id(&self.snapshot, &self.filtered, row)
                            {
                                core.enqueue(vec![id]);
                            }
                        }
                        Message::QueueRowClicked(row) => {
                            if let Some(index) = displayed_queue_index(&self.snapshot, row) {
                                core.play_queue_index(index);
                            }
                        }
                        Message::QueueMove(from, to) => {
                            if let Some(core) = &self.controller {
                                core.move_queue(from, to);
                            }
                        }
                        Message::QueueRemove(row) => {
                            if let Some(index) = displayed_queue_index(&self.snapshot, row) {
                                core.remove_queue_index(index);
                            }
                        }
                        Message::TogglePlay => core.toggle_play(),
                        Message::Next => core.next(),
                        Message::Previous => core.previous(),
                        Message::Stop => core.stop(),
                        Message::SeekCommit => {
                            if let Some(position) = self.seek_preview.take() {
                                core.seek(position);
                            }
                        }
                        Message::VolumeChanged(volume) => core.set_volume(volume),
                        Message::ToggleShuffle => core.toggle_shuffle(),
                        Message::CycleRepeat => core.cycle_repeat(),
                        _ => {}
                    }
                }
            }
        }
        Task::none()
    }
}
