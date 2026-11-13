use super::*;
use console_link::nintendo::installation::{self, Cancellation, Stage};
use std::sync::mpsc::{self, Receiver, TryRecvError};

enum Update {
    Stage(Stage),
    Finished(Result<(), String>),
}

#[derive(Default)]
enum Status {
    #[default]
    Unknown,
    Missing,
    Installing(Stage),
    Ready,
    Failed(String),
}

#[derive(Default)]
pub(super) struct ComponentInstall {
    status: Status,
    pending: Option<Receiver<Update>>,
    cancelled: Arc<Cancellation>,
}

impl ComponentInstall {
    pub(super) fn poll(&mut self) -> bool {
        let mut installed = false;
        loop {
            match self.pending.as_ref().map(Receiver::try_recv) {
                Some(Ok(Update::Stage(stage))) => self.status = Status::Installing(stage),
                Some(Ok(Update::Finished(result))) => {
                    installed = result.is_ok();
                    self.status = match result {
                        Ok(()) => Status::Ready,
                        Err(error) => Status::Failed(error),
                    };
                    self.pending = None;
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    self.pending = None;
                    self.status = Status::Failed("Установка прервана. Можно повторить.".into());
                }
                _ => break,
            }
        }
        installed
    }

    fn start(&mut self, ctx: egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let base = match console_link::nintendo::helper_directory() {
            Ok(base) => base,
            Err(error) => {
                self.status = Status::Failed(error);
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.cancelled = Arc::new(Cancellation::default());
        let cancelled = Arc::clone(&self.cancelled);
        self.status = Status::Installing(Stage::Downloading);
        std::thread::spawn(move || {
            let result = installation::install(
                &base,
                |stage| {
                    let _ = tx.send(Update::Stage(stage));
                    ctx.request_repaint();
                },
                &cancelled,
            )
            .map(|_| ());
            let _ = tx.send(Update::Finished(result));
            ctx.request_repaint();
        });
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        if matches!(self.status, Status::Unknown) {
            self.status = if console_link::nintendo::helper_directory()
                .ok()
                .and_then(|base| installation::installed(&base))
                .is_some()
            {
                Status::Ready
            } else {
                Status::Missing
            };
        }
        match &self.status {
            Status::Installing(stage) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(stage.label());
                });
            }
            Status::Ready => theme::subtitle(ui, "Компоненты Nintendo установлены."),
            Status::Failed(error) => theme::subtitle(ui, error),
            _ => {}
        }
        if self.pending.is_none() {
            let label = if matches!(self.status, Status::Ready) {
                "Проверить/восстановить компоненты"
            } else {
                "Установить компоненты Nintendo"
            };
            if ui.button(label).clicked() {
                self.start(ui.ctx().clone());
            }
        }
        theme::subtitle(
            ui,
            "Node.js и nxapi устанавливаются только для Console Link. Нужен интернет; системный Node не меняется. Вход в Nintendo настраивается отдельно.",
        );
    }
}

impl Drop for ComponentInstall {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_survives_a_closed_progress_channel() {
        let (tx, rx) = mpsc::channel();
        let mut state = ComponentInstall::default();
        state.pending = Some(rx);
        tx.send(Update::Stage(Stage::Installing)).unwrap();
        tx.send(Update::Finished(Ok(()))).unwrap();
        drop(tx);
        assert!(state.poll());
        assert!(matches!(state.status, Status::Ready));
        assert!(state.pending.is_none());
        assert!(!state.poll());
    }

    #[test]
    fn interrupted_worker_does_not_leave_an_infinite_spinner() {
        let (tx, rx) = mpsc::channel();
        let mut state = ComponentInstall::default();
        state.pending = Some(rx);
        drop(tx);
        assert!(!state.poll());
        assert!(matches!(state.status, Status::Failed(_)));
        assert!(state.pending.is_none());
    }
}
