use crate::{Constellations, Message, QrLoginStep};
use cosmic::{
    Element,
    iced::Alignment,
    widget::{Column, button, container, text, text_input, tooltip, tooltip::Position},
};

impl Constellations {
    pub fn view_login(&self) -> Element<'_, Message> {
        if self.is_qr_logging_in {
            return self.view_qr_login();
        }
        let title = if self.is_registering_mode {
            crate::fl!("register-title")
        } else {
            crate::fl!("login-title")
        };
        let mut content = Column::new()
            .spacing(10)
            .padding(20)
            .max_width(400)
            .align_x(Alignment::Center)
            .push(text::title1(title));

        let homeserver_input = text_input(crate::fl!("homeserver"), &self.login_homeserver);
        let username_input = text_input(crate::fl!("username"), &self.login_username);
        let password_input = text_input(crate::fl!("password"), &self.login_password).password();

        let (homeserver_input, username_input, password_input) =
            if self.is_logging_in || self.is_oidc_logging_in || self.is_registering {
                (homeserver_input, username_input, password_input)
            } else {
                (
                    homeserver_input.on_input(Message::LoginHomeserverChanged),
                    username_input.on_input(Message::LoginUsernameChanged),
                    password_input
                        .on_input(Message::LoginPasswordChanged)
                        .on_submit(|_| {
                            if self.is_registering_mode {
                                Message::SubmitRegister
                            } else {
                                Message::SubmitLogin
                            }
                        }),
                )
            };

        content = content
            .push(homeserver_input)
            .push(username_input)
            .push(password_input);

        let is_missing_fields = self.login_homeserver.trim().is_empty()
            || self.login_username.trim().is_empty()
            || self.login_password.is_empty();

        let main_button: Element<'_, Message> = if self.is_registering_mode {
            if self.is_registering {
                button::text(crate::fl!("creating-account")).into()
            } else {
                let mut btn = button::text(crate::fl!("create-account-button"));
                if !is_missing_fields {
                    btn = btn.on_press(Message::SubmitRegister);
                }
                if is_missing_fields {
                    tooltip(
                        btn,
                        text::body(crate::fl!("fill-all-fields-register")),
                        Position::Top,
                    )
                    .into()
                } else {
                    btn.into()
                }
            }
        } else if self.is_logging_in {
            button::text(crate::fl!("logging-in")).into()
        } else {
            let mut btn = button::text(crate::fl!("login-button"));
            if !is_missing_fields && !self.is_oidc_logging_in {
                btn = btn.on_press(Message::SubmitLogin);
            }
            if is_missing_fields {
                tooltip(
                    btn,
                    text::body(crate::fl!("fill-all-fields-login")),
                    Position::Top,
                )
                .into()
            } else {
                btn.into()
            }
        };

        let oidc_button: Element<'_, Message> = if self.is_oidc_logging_in {
            let oidc_col = Column::new()
                .spacing(5)
                .align_x(Alignment::Center)
                .push(text::body(crate::fl!("waiting-for-browser")))
                .push(button::text(crate::fl!("cancel")).on_press(Message::CancelOidcLogin));
            oidc_col.into()
        } else {
            let mut btn = button::text(crate::fl!("oidc-login-button"));
            if !self.login_homeserver.is_empty() && !self.is_logging_in && !self.is_registering_mode
            {
                btn = btn.on_press(Message::SubmitOidcLogin);
            }
            btn.into()
        };

        let qr_login_button = {
            let mut btn = button::text(crate::fl!("login-qr-button"));
            if !self.is_logging_in && !self.is_registering_mode && !self.is_oidc_logging_in {
                btn = btn.on_press(Message::StartQrLogin);
            }
            btn
        };

        let toggle_mode_button = if self.is_registering_mode {
            button::text(crate::fl!("already-have-account"))
        } else {
            button::text(crate::fl!("need-account"))
        };

        let toggle_mode_button =
            if self.is_logging_in || self.is_registering || self.is_oidc_logging_in {
                toggle_mode_button
            } else {
                toggle_mode_button.on_press(Message::ToggleLoginMode)
            };

        content = content.push(main_button);

        if !self.is_registering_mode {
            content = content.push(oidc_button);
            content = content.push(qr_login_button);
        }

        content = content.push(toggle_mode_button);

        container(content)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    pub fn view_qr_login(&self) -> Element<'_, Message> {
        let title = crate::fl!("login-qr-title");
        let mut content = Column::new()
            .spacing(15)
            .padding(20)
            .max_width(450)
            .align_x(Alignment::Center)
            .push(text::title1(title));

        match self.qr_login_step {
            QrLoginStep::Initiating => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-initiating")));
            }
            QrLoginStep::ShowingQr => {
                content = content.push(text::body(crate::fl!("login-qr-scanning")));

                if let Some(ref url) = self.qr_rendezvous_url
                    && let Ok(code) = qrcode::QrCode::new(url.as_bytes())
                {
                    let width = code.width();
                    let scale = 8;
                    let scaled_width = width * scale;
                    let mut pixels = Vec::with_capacity(scaled_width * scaled_width * 4);
                    for y in 0..scaled_width {
                        let qr_y = y / scale;
                        for x in 0..scaled_width {
                            let qr_x = x / scale;
                            let is_dark = code[(qr_x, qr_y)] == qrcode::Color::Dark;
                            let color = if is_dark {
                                [0, 0, 0, 255] // Black
                            } else {
                                [255, 255, 255, 255] // White
                            };
                            pixels.extend_from_slice(&color);
                        }
                    }

                    let handle = cosmic::iced::widget::image::Handle::from_rgba(
                        scaled_width as u32,
                        scaled_width as u32,
                        pixels,
                    );

                    content = content.push(
                        container(
                            cosmic::widget::image(handle)
                                .width(cosmic::iced::Length::Fixed(200.0))
                                .height(cosmic::iced::Length::Fixed(200.0)),
                        )
                        .padding(15),
                    );
                }
            }
            QrLoginStep::RendezvousEstablished => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-established")));
            }
            QrLoginStep::Authenticating => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-authenticating")));
            }
            QrLoginStep::Success => {
                content = content.push(text::body(crate::fl!("login-qr-success")));
            }
            _ => {
                content = content.push(text::body("An error occurred."));
            }
        }

        let cancel_btn =
            button::text(crate::fl!("login-qr-cancel")).on_press(Message::CancelQrLogin);
        content = content.push(cancel_btn);

        container(content)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
