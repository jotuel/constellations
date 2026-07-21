use crate::{Constellations, MediaSource, Message};
use cosmic::{Action, Application, Task};

impl Constellations {
    pub fn handle_fetch_media(
        &mut self,
        source: MediaSource,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        if let Some(matrix) = &self.matrix {
            let matrix = matrix.clone();
            let mxc_url = match &source {
                MediaSource::Plain(uri) => uri.to_string(),
                MediaSource::Encrypted(file) => file.url.to_string(),
            };
            Task::perform(
                async move { matrix.fetch_media(source).await.map_err(|e| e.to_string()) },
                move |res| Action::from(Message::MediaFetched(mxc_url, res)),
            )
        } else {
            Task::none()
        }
    }

    pub fn handle_media_fetched(
        &mut self,
        mxc_url: String,
        res: Result<Vec<u8>, String>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        match res {
            Ok(data) => {
                self.media_cache.insert(
                    mxc_url,
                    cosmic::iced::widget::image::Handle::from_bytes(data),
                );
            }
            Err(e) => {
                self.set_error(
                    crate::fl!("error-failed-fetch-media", error = e.to_string()).to_string(),
                );
            }
        }
        Task::none()
    }

    pub fn handle_media_fetched_batch(
        &mut self,
        batch: Vec<(String, Result<Vec<u8>, String>)>,
    ) -> Task<Action<<Constellations as Application>::Message>> {
        for (mxc_url, res) in batch {
            match res {
                Ok(data) => {
                    self.media_cache.insert(
                        mxc_url,
                        cosmic::iced::widget::image::Handle::from_bytes(data),
                    );
                }
                Err(e) => {
                    self.set_error(
                        crate::fl!("error-failed-fetch-media", error = e.to_string()).to_string(),
                    );
                }
            }
        }
        Task::none()
    }
}
