use super::*;
use std::collections::BTreeSet;

pub(super) struct Navigation {
    tab: usize,
    query: String,
    view: Option<workflow::View>,
    today: bool,
    id: Option<String>,
    scroll: u16,
}
pub(super) enum Panel {
    Related {
        rows: Vec<Value>,
        selected: usize,
    },
    Merge {
        target: String,
        rows: Vec<Value>,
        selected: usize,
        preview: Option<Value>,
        baseline: Vec<Value>,
        scroll: u16,
    },
    Bulk {
        step: u8,
        field: String,
        input: Editor,
    },
}
impl App {
    pub(super) fn action_key(&mut self, store: &Store, key: KeyEvent) -> Result<bool> {
        if let Some(mut panel) = self.action_panel.take() {
            if key.code == KeyCode::Esc
                || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g'))
            {
                return Ok(true);
            }
            let mut close = false;
            let result = (|| -> Result<()> {
                match &mut panel {
                    Panel::Related { rows, selected } => match key.code {
                        KeyCode::Enter => {
                            if let Some(r) = rows.get(*selected) {
                                self.jump_record(r);
                                close = true;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('n') => {
                            *selected = (*selected + 1).min(rows.len().saturating_sub(1))
                        }
                        KeyCode::Up | KeyCode::Char('p') => *selected = selected.saturating_sub(1),
                        _ => {}
                    },
                    Panel::Merge {
                        target,
                        rows,
                        selected,
                        preview,
                        baseline,
                        scroll,
                    } => match key.code {
                        KeyCode::Enter if preview.is_none() => {
                            if let Some(r) = rows.get(*selected) {
                                *preview = Some(store.merge_records(
                                    target,
                                    r["id"].as_str().unwrap(),
                                    true,
                                )?);
                                *baseline = store.all_records()?;
                            }
                        }
                        KeyCode::F(10) if preview.is_some() => {
                            if *baseline != store.all_records()? {
                                bail!("Records changed; reopen merge and review again");
                            }
                            let r = &rows[*selected];
                            store.merge_records(target, r["id"].as_str().unwrap(), false)?;
                            self.refresh(store)?;
                            self.marked.remove(r["id"].as_str().unwrap());
                            self.message="Records merged; source values and history retained in the archive.".into();
                            close = true;
                        }
                        KeyCode::Down | KeyCode::Char('n') => {
                            if preview.is_some() {
                                *scroll = scroll.saturating_add(1)
                            } else {
                                *selected = (*selected + 1).min(rows.len().saturating_sub(1))
                            }
                        }
                        KeyCode::Up | KeyCode::Char('p') => {
                            if preview.is_some() {
                                *scroll = scroll.saturating_sub(1)
                            } else {
                                *selected = selected.saturating_sub(1)
                            }
                        }
                        _ => {}
                    },
                    Panel::Bulk { step, field, input } => match key.code {
                        KeyCode::Char('u') if *step == 0 => {
                            *step = 1;
                            input.clear();
                        }
                        KeyCode::Char('d') if *step == 0 => *step = 4,
                        KeyCode::Char('e') if *step == 0 => {
                            *step = 3;
                            input.clear();
                        }
                        KeyCode::Enter if *step == 1 => {
                            let f = input.text().trim().to_owned();
                            if f.is_empty() {
                                bail!("Enter a field name");
                            }
                            *field = f;
                            *step = 2;
                            input.clear();
                        }
                        KeyCode::Enter if *step == 2 => {
                            let text = input.text();
                            let value =
                                serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
                            let patch = json!({field.clone():value});
                            let ids: Vec<_> = self.marked.iter().cloned().collect();
                            store.bulk("update", &ids, &patch)?;
                            self.refresh(store)?;
                            self.message = format!("Updated {} selected records", ids.len());
                            close = true;
                        }
                        KeyCode::Enter if *step == 3 => {
                            let rows = store.bulk(
                                "export",
                                &self.marked.iter().cloned().collect::<Vec<_>>(),
                                &json!({}),
                            )?;
                            use std::io::Write;
                            let mut file = std::fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(input.text().trim())
                                .context("Choose a new export file path")?;
                            file.write_all(serde_json::to_string_pretty(&rows)?.as_bytes())?;
                            self.message =
                                format!("Exported {} selected records", self.marked.len());
                            close = true;
                        }
                        KeyCode::Char('y') if *step == 4 && key.modifiers.is_empty() => {
                            let ids: Vec<_> = self.marked.iter().cloned().collect();
                            store.bulk("trash", &ids, &json!({}))?;
                            self.marked.clear();
                            self.refresh(store)?;
                            self.message = format!("Moved {} records to trash", ids.len());
                            close = true;
                        }
                        _ if [1, 2, 3].contains(step) => input.handle(key),
                        _ => {}
                    },
                }
                Ok(())
            })();
            if let Err(e) = result {
                self.message = format!("Error: {e}");
            }
            if !close {
                self.action_panel = Some(panel);
            }
            return Ok(true);
        }
        if !key.modifiers.is_empty()
            || self.prefix
            || self.mode != Mode::Browse
            || self.form.is_some()
            || self.tool.is_some()
            || self.view_manager.is_some()
            || self.timeline.is_some()
            || self.help
        {
            return Ok(false);
        }
        match key.code {
            KeyCode::Char(' ') => {
                if let Some(r) = self.current() {
                    let id = r["id"].as_str().unwrap().to_owned();
                    if !self.marked.remove(&id) {
                        self.marked.insert(id);
                    }
                    self.message = format!(
                        "{} selected • Space toggles • A selects visible • U bulk actions",
                        self.marked.len()
                    );
                }
            }
            KeyCode::Char('A') => {
                self.marked.extend(
                    self.visible()
                        .iter()
                        .filter_map(|r| r["id"].as_str().map(str::to_owned))
                        .collect::<Vec<_>>(),
                );
                self.message = format!(
                    "{} selected • U bulk actions • C clears selection",
                    self.marked.len()
                );
            }
            KeyCode::Char('C') => {
                self.marked.clear();
                self.message = "Selection cleared".into();
            }
            KeyCode::Char('U') => {
                if self.marked.is_empty() {
                    self.message = "Space selects records; A selects all visible records.".into();
                } else {
                    self.action_panel = Some(Panel::Bulk {
                        step: 0,
                        field: String::new(),
                        input: Editor::default(),
                    });
                }
            }
            KeyCode::Char('r') => {
                if let Some(r) = self.current() {
                    self.action_panel = Some(Panel::Related {
                        rows: store.related_records(r["id"].as_str().unwrap())?,
                        selected: 0,
                    });
                }
            }
            KeyCode::Char('B') => {
                if let Some(n) = self.navigation.pop() {
                    self.tab = n.tab;
                    self.query = n.query;
                    self.active_view = n.view;
                    self.today = n.today;
                    self.selected = self
                        .visible()
                        .iter()
                        .position(|r| r["id"].as_str() == n.id.as_deref())
                        .unwrap_or(0);
                    self.scroll = n.scroll;
                } else {
                    self.message = "No previous relationship location".into();
                }
            }
            KeyCode::Char('M') => {
                if let Some(target) = self.current() {
                    let rows = self
                        .records
                        .iter()
                        .filter(|r| r["kind"] == target["kind"] && r["id"] != target["id"])
                        .cloned()
                        .collect();
                    self.action_panel = Some(Panel::Merge {
                        target: target["id"].as_str().unwrap().into(),
                        rows,
                        selected: 0,
                        preview: None,
                        baseline: vec![],
                        scroll: 0,
                    });
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn jump_record(&mut self, r: &Value) {
        self.navigation.push(Navigation {
            tab: self.tab,
            query: self.query.clone(),
            view: self.active_view.clone(),
            today: self.today,
            id: self
                .current()
                .and_then(|r| r["id"].as_str().map(str::to_owned)),
            scroll: self.scroll,
        });
        self.today = false;
        self.active_view = None;
        self.query.clear();
        self.tab = self
            .kinds
            .iter()
            .position(|k| r["kind"] == k.as_str())
            .unwrap_or(0);
        self.selected = self
            .visible()
            .iter()
            .position(|v| v["id"] == r["id"])
            .unwrap_or(0);
        self.scroll = 0;
        self.message = "r: related records • B: back".into();
    }
    pub(super) fn render_actions(&self, frame: &mut Frame) {
        let Some(panel) = &self.action_panel else {
            return;
        };
        let p = self.theme.palette();
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
        let (title, help) = match panel {
            Panel::Related { rows, selected } => {
                render_rows(frame, areas[0], p, rows, *selected, " Related records ");
                (
                    "",
                    "↑/↓ or n/p: select • Enter: follow • Esc: close • B returns after following",
                )
            }
            Panel::Merge {
                rows,
                selected,
                preview,
                scroll,
                ..
            } => {
                if let Some(preview) = preview {
                    let text = format!(
                        "Survivor keeps its name and conflicting values. Missing values are filled; lists are combined.\n{} linked records will be redirected.\nSource is archived with its original data; history is retained.\n\nConflicts (kept / archived):\n{}\n\nResulting fields:\n{}",
                        preview["redirected"],
                        preview["conflicts"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|c| format!(
                                "{}: {} / {}",
                                c["field"].as_str().unwrap_or(""),
                                workflow::friendly(&c["kept"], &self.records),
                                workflow::friendly(&c["archived"], &self.records)
                            ))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        preview["data"]
                            .as_object()
                            .unwrap()
                            .iter()
                            .map(|(k, v)| format!(
                                "{}: {}",
                                details::label(k),
                                workflow::friendly(v, &self.records)
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    frame.render_widget(
                        Paragraph::new(text)
                            .wrap(Wrap { trim: false })
                            .scroll((*scroll, 0))
                            .style(p.base())
                            .block(Block::bordered().title(" Review merge ")),
                        areas[0],
                    );
                } else {
                    render_rows(
                        frame,
                        areas[0],
                        p,
                        rows,
                        *selected,
                        " Choose duplicate to merge into current record ",
                    );
                }
                (
                    "",
                    "Enter: preview • F10: commit reviewed merge • ↑/↓: select/scroll • Esc: cancel",
                )
            }
            Panel::Bulk { step, field, input } => {
                let prompt = match step {
                    0 => format!(
                        "{} records selected (including selections outside the current filter).\n\nu: update a field\nd: move to trash\ne: export JSON",
                        self.marked.len()
                    ),
                    1 => "Field name to update (name renames records):".into(),
                    2 => format!("Value for {field} (plain text or a JSON number, boolean, list):"),
                    3 => "New output file path (existing files are preserved):".into(),
                    _ => format!(
                        "Move {} selected records to trash? Press y to confirm.",
                        self.marked.len()
                    ),
                };
                frame.render_widget(
                    Paragraph::new(prompt)
                        .style(p.base())
                        .wrap(Wrap { trim: false })
                        .block(Block::bordered().title(" Bulk actions ")),
                    areas[0],
                );
                if [1, 2, 3].contains(step) && areas[0].height > 3 && areas[0].width > 4 {
                    let input_area = ratatui::layout::Rect::new(
                        areas[0].x + 1,
                        areas[0].bottom() - 2,
                        areas[0].width - 2,
                        1,
                    );
                    let col =
                        Line::from(input.chars[..input.cursor].iter().collect::<String>()).width();
                    let offset = col.saturating_sub(input_area.width.saturating_sub(1) as usize);
                    frame.render_widget(
                        Paragraph::new(input.text())
                            .scroll((0, offset.min(u16::MAX as usize) as u16))
                            .style(p.base()),
                        input_area,
                    );
                    frame.set_cursor_position((input_area.x + (col - offset) as u16, input_area.y));
                }
                ("", "Enter: apply / continue • Esc: cancel")
            }
        };
        let _ = title;
        frame.render_widget(
            Paragraph::new(format!("{help}\n{}", self.message))
                .style(p.base())
                .wrap(Wrap { trim: false }),
            areas[1],
        );
    }
}
fn render_rows(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    p: theme::Palette,
    rows: &[Value],
    selected: usize,
    title: &str,
) {
    let labels: Vec<_> = if rows.is_empty() {
        vec!["No matching records".into()]
    } else {
        rows.iter()
            .map(|r| {
                format!(
                    "{} • {} • {}",
                    r["name"].as_str().unwrap_or(""),
                    r["kind"].as_str().unwrap_or(""),
                    r["id"].as_str().unwrap_or("")
                )
            })
            .collect()
    };
    frame.render_stateful_widget(
        List::new(labels)
            .style(p.base())
            .highlight_symbol("› ")
            .highlight_style(p.base().bg(p.selection))
            .block(Block::bordered().title(title)),
        area,
        &mut ListState::default().with_selected(Some(selected)),
    );
}
pub(super) fn empty_selection() -> BTreeSet<String> {
    BTreeSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(a: &mut App, s: &Store, c: char) {
        a.handle(s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
    fn enter(a: &mut App, s: &Store) {
        a.handle(s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
    }
    #[test]
    fn changed_records_block_a_reviewed_merge() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let a = s
            .execute(&["add".into(), "contact".into(), "A".into()])
            .unwrap();
        let b = s
            .execute(&["add".into(), "contact".into(), "B".into()])
            .unwrap();
        let mut app = App::new(&s, None).unwrap();
        app.category(1);
        app.selected = app
            .visible()
            .iter()
            .position(|r| r["id"] == a["id"])
            .unwrap();
        key(&mut app, &s, 'M');
        enter(&mut app, &s);
        s.execute(&[
            "update".into(),
            b["id"].as_str().unwrap().into(),
            json!({"phone":"changed"}).to_string(),
        ])
        .unwrap();
        app.handle(&s, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert!(app.message.contains("Records changed"));
        assert_eq!(s.all_records().unwrap().len(), 2);
    }
    #[test]
    fn follow_back_bulk_selection_update_export_and_reviewed_merge() {
        let s = Store::open(std::path::Path::new(":memory:")).unwrap();
        let company = s
            .execute(&["add".into(), "company".into(), "Acme".into()])
            .unwrap();
        let a = s
            .execute(&[
                "add".into(),
                "contact".into(),
                "Ada".into(),
                json!({"company_id":company["id"]}).to_string(),
            ])
            .unwrap();
        let b = s
            .execute(&[
                "add".into(),
                "contact".into(),
                "Duplicate".into(),
                json!({"phone":"123"}).to_string(),
            ])
            .unwrap();
        let mut app = App::new(&s, None).unwrap();
        app.category(1);
        app.query = "Ada".into();
        key(&mut app, &s, 'r');
        enter(&mut app, &s);
        assert_eq!(app.current().unwrap()["id"], company["id"]);
        key(&mut app, &s, 'B');
        assert_eq!(app.current().unwrap()["id"], a["id"]);
        assert_eq!(app.query, "Ada");
        key(&mut app, &s, ' ');
        app.query.clear();
        key(&mut app, &s, 'A');
        assert_eq!(app.marked.len(), 2);
        key(&mut app, &s, 'U');
        key(&mut app, &s, 'u');
        for c in "city".chars() {
            key(&mut app, &s, c);
        }
        enter(&mut app, &s);
        for c in "London".chars() {
            key(&mut app, &s, c);
        }
        enter(&mut app, &s);
        assert_eq!(
            s.get(b["id"].as_str().unwrap()).unwrap()["data"]["city"],
            "London"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("selected.json");
        key(&mut app, &s, 'U');
        key(&mut app, &s, 'e');
        for c in path.to_str().unwrap().chars() {
            key(&mut app, &s, c);
        }
        enter(&mut app, &s);
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap())
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        app.selected = app
            .visible()
            .iter()
            .position(|r| r["id"] == a["id"])
            .unwrap();
        key(&mut app, &s, 'M');
        enter(&mut app, &s);
        assert!(s.get(b["id"].as_str().unwrap()).is_ok());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("Review merge"));
        app.handle(&s, KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))
            .unwrap();
        assert!(app.action_panel.is_none());
        assert!(s.get(b["id"].as_str().unwrap()).is_err());
        assert_eq!(
            s.get(a["id"].as_str().unwrap()).unwrap()["data"]["phone"],
            "123"
        );
        key(&mut app, &s, 'U');
        key(&mut app, &s, 'd');
        assert!(s.trash().unwrap().is_empty());
        key(&mut app, &s, 'y');
        assert_eq!(s.trash().unwrap().len(), 1);
    }
}
