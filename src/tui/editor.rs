use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[derive(Default)]
pub struct Editor {
    pub chars: Vec<char>,
    pub cursor: usize,
    kill: Vec<char>,
    pub history: Vec<String>,
    history_index: Option<usize>,
    draft: String,
}
impl Editor {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }
    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.history_index = None;
    }
    pub fn set(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }
    pub fn submit(&mut self) -> String {
        let s = self.text();
        if !s.trim().is_empty() && self.history.last() != Some(&s) {
            self.history.push(s.clone());
        }
        self.clear();
        s
    }
    fn backward_word(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }
    fn forward_word(&self) -> usize {
        let mut i = self.cursor;
        while i < self.chars.len() && self.chars[i].is_whitespace() {
            i += 1;
        }
        while i < self.chars.len() && !self.chars[i].is_whitespace() {
            i += 1;
        }
        i
    }
    pub fn handle(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.chars.len(),
            KeyCode::Char('b') if alt => self.cursor = self.backward_word(),
            KeyCode::Char('f') if alt => self.cursor = self.forward_word(),
            KeyCode::Char('b') if ctrl => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('f') if ctrl => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            KeyCode::Char('k') if ctrl => self.kill = self.chars.drain(self.cursor..).collect(),
            KeyCode::Char('u') if ctrl => {
                self.kill = self.chars.drain(..self.cursor).collect();
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let start = self.backward_word();
                self.kill = self.chars.drain(start..self.cursor).collect();
                self.cursor = start;
            }
            KeyCode::Char('d') if alt => {
                let end = self.forward_word();
                self.kill = self.chars.drain(self.cursor..end).collect();
            }
            KeyCode::Char('y') if ctrl => {
                self.chars
                    .splice(self.cursor..self.cursor, self.kill.clone());
                self.cursor += self.kill.len();
            }
            KeyCode::Char('d') if ctrl => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
            }
            KeyCode::Char('p') if ctrl => self.previous(),
            KeyCode::Char('n') if ctrl => self.next(),
            KeyCode::Up => self.previous(),
            KeyCode::Down => self.next(),
            KeyCode::Char(c) if !ctrl && !alt => {
                self.chars.insert(self.cursor, c);
                self.cursor += 1;
            }
            _ => {}
        }
    }
    fn previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let i = match self.history_index {
            None => {
                self.draft = self.text();
                self.history.len() - 1
            }
            Some(i) => i.saturating_sub(1),
        };
        let s = self.history[i].clone();
        self.set(&s);
        self.history_index = Some(i);
    }
    fn next(&mut self) {
        if let Some(i) = self.history_index {
            if i + 1 < self.history.len() {
                let s = self.history[i + 1].clone();
                self.set(&s);
                self.history_index = Some(i + 1);
            } else {
                let s = self.draft.clone();
                self.set(&s);
                self.history_index = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(e: &mut Editor, c: char, m: KeyModifiers) {
        e.handle(KeyEvent::new(KeyCode::Char(c), m));
    }
    #[test]
    fn unicode_edit_kill_yank_and_history() {
        let mut e = Editor::default();
        e.set("Ada 東京");
        key(&mut e, 'b', KeyModifiers::ALT);
        key(&mut e, 'w', KeyModifiers::CONTROL);
        assert_eq!(e.text(), "東京");
        key(&mut e, 'y', KeyModifiers::CONTROL);
        assert_eq!(e.text(), "Ada 東京");
        key(&mut e, 'a', KeyModifiers::CONTROL);
        key(&mut e, 'd', KeyModifiers::CONTROL);
        assert_eq!(e.text(), "da 東京");
        e.submit();
        e.set("draft");
        key(&mut e, 'p', KeyModifiers::CONTROL);
        assert_eq!(e.text(), "da 東京");
        key(&mut e, 'n', KeyModifiers::CONTROL);
        assert_eq!(e.text(), "draft");
    }
}
