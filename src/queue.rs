use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub path: PathBuf,
}

#[derive(Default)]
pub struct Queue {
    pub entries: Vec<Entry>,
    pub selected: Option<u64>,
    next_id: u64,
}

impl Queue {
    pub fn append(&mut self, path: PathBuf) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(Entry { id, path });
        self.selected.get_or_insert(id);
        id
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| Some(e.id) == self.selected)
    }

    pub fn select_relative(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = None;
            return;
        }
        let index = self.selected_index().unwrap_or(0);
        let next = index
            .saturating_add_signed(delta)
            .min(self.entries.len() - 1);
        self.selected = Some(self.entries[next].id);
    }

    pub fn move_selected(&mut self, delta: isize) -> bool {
        let Some(index) = self.selected_index() else {
            return false;
        };
        let next = index
            .saturating_add_signed(delta)
            .min(self.entries.len() - 1);
        if next == index {
            return false;
        }
        let entry = self.entries.remove(index);
        self.entries.insert(next, entry);
        true
    }

    pub fn move_to_gap(&mut self, id: u64, gap: usize) {
        let Some(index) = self.entries.iter().position(|e| e.id == id) else {
            return;
        };
        let gap = gap.min(self.entries.len());
        let target = if gap > index { gap - 1 } else { gap };
        let entry = self.entries.remove(index);
        self.entries.insert(target, entry);
        self.selected = Some(id);
    }

    pub fn remove_selected(&mut self) -> Option<Entry> {
        let index = self.selected_index()?;
        let removed = self.entries.remove(index);
        self.selected = self
            .entries
            .get(index)
            .or_else(|| self.entries.last())
            .map(|e| e.id);
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_files_have_separate_identity_and_selection_follows_moves() {
        let mut q = Queue::default();
        let first = q.append("clip.mp4".into());
        let second = q.append("clip.mp4".into());
        assert_ne!(first, second);
        q.selected = Some(second);
        q.move_selected(-1);
        assert_eq!(q.entries[0].id, second);
        assert_eq!(q.selected_index(), Some(0));
        assert!(!q.move_selected(-1));
    }

    #[test]
    fn drop_gap_accounts_for_removed_row_and_last_row() {
        let mut q = Queue::default();
        let ids: Vec<_> = (0..4)
            .map(|n| q.append(format!("{n}.mp4").into()))
            .collect();
        q.move_to_gap(ids[0], 4);
        assert_eq!(
            q.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[1], ids[2], ids[3], ids[0]]
        );
        q.move_to_gap(ids[0], 0);
        assert_eq!(q.selected_index(), Some(0));
        q.remove_selected();
        assert_eq!(q.selected, Some(ids[1]));
    }
}
