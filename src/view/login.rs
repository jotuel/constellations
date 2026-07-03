use crate::{AuthFlow, Constellations, Message, QrLoginStep};
use cosmic::{
    Element,
    iced::Alignment,
    widget::{Column, button, container, text, text_input, tooltip, tooltip::Position},
};

impl Constellations {
    pub fn view_login(&self) -> Element<'_, Message> {
        if matches!(self.auth_flow, AuthFlow::Qr { .. }) {
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
            if (self.auth_flow == AuthFlow::Password || self.auth_flow == AuthFlow::Oidc)
                || self.is_registering
            {
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
        } else if self.auth_flow == AuthFlow::Password {
            button::text(crate::fl!("logging-in")).into()
        } else {
            let mut btn = button::text(crate::fl!("login-button"));
            if !is_missing_fields && self.auth_flow != AuthFlow::Oidc {
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

        let oidc_button: Element<'_, Message> = if self.auth_flow == AuthFlow::Oidc {
            let oidc_col = Column::new()
                .spacing(5)
                .align_x(Alignment::Center)
                .push(text::body(crate::fl!("waiting-for-browser")))
                .push(button::text(crate::fl!("cancel")).on_press(Message::CancelOidcLogin));
            oidc_col.into()
        } else {
            let mut btn = button::text(crate::fl!("oidc-login-button"));
            if !self.login_homeserver.is_empty()
                && self.auth_flow != AuthFlow::Password
                && !self.is_registering_mode
            {
                btn = btn.on_press(Message::SubmitOidcLogin);
            }
            btn.into()
        };

        let qr_login_button = {
            let mut btn = button::text(crate::fl!("login-qr-button"));
            if self.auth_flow != AuthFlow::Password
                && !self.is_registering_mode
                && self.auth_flow != AuthFlow::Oidc
            {
                btn = btn.on_press(Message::StartQrLogin);
            }
            btn
        };

        let toggle_mode_button = if self.is_registering_mode {
            button::text(crate::fl!("already-have-account"))
        } else {
            button::text(crate::fl!("need-account"))
        };

        let toggle_mode_button = if (self.auth_flow == AuthFlow::Password
            || self.auth_flow == AuthFlow::Oidc)
            || self.is_registering
        {
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
        let step = match self.auth_flow {
            AuthFlow::Qr { step } => step,
            _ => QrLoginStep::NotStarted,
        };
        let mut content = Column::new()
            .spacing(15)
            .padding(20)
            .max_width(450)
            .align_x(Alignment::Center)
            .push(text::title1(title));

        match step {
            QrLoginStep::Initiating => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-initiating")));
            }
            QrLoginStep::ShowingQr => {
                content = content.push(text::body(crate::fl!("login-qr-scanning")));

                if let Some(ref url) = self.qr_rendezvous_url {
                    content = content.push(container(QrCodeWidget::new(url.clone())).padding(15));
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

pub struct QrCodeWidget {
    url: String,
}

impl QrCodeWidget {
    pub fn new(url: String) -> Self {
        Self { url }
    }
}

impl<Message, Renderer> cosmic::iced::advanced::Widget<Message, cosmic::Theme, Renderer>
    for QrCodeWidget
where
    Renderer: cosmic::iced::advanced::Renderer,
{
    fn size(&self) -> cosmic::iced::Size<cosmic::iced::Length> {
        cosmic::iced::Size::new(
            cosmic::iced::Length::Fixed(200.0),
            cosmic::iced::Length::Fixed(200.0),
        )
    }

    fn layout(
        &mut self,
        _tree: &mut cosmic::iced::advanced::widget::Tree,
        _renderer: &Renderer,
        _limits: &cosmic::iced::advanced::layout::Limits,
    ) -> cosmic::iced::advanced::layout::Node {
        cosmic::iced::advanced::layout::Node::new(cosmic::iced::Size::new(200.0, 200.0))
    }

    fn draw(
        &self,
        _state: &cosmic::iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        _theme: &cosmic::Theme,
        _style: &cosmic::iced::advanced::renderer::Style,
        layout: cosmic::iced::advanced::layout::Layout<'_>,
        _cursor: cosmic::iced::advanced::mouse::Cursor,
        _viewport: &cosmic::iced::Rectangle,
    ) {
        let bounds = layout.bounds();

        // Draw white background
        renderer.fill_quad(
            cosmic::iced::advanced::renderer::Quad {
                bounds,
                border: cosmic::iced::Border::default(),
                shadow: cosmic::iced::Shadow::default(),
                snap: false,
            },
            cosmic::iced::Color::WHITE,
        );

        if let Ok(code) = qrcode::QrCode::new(self.url.as_bytes()) {
            let width = code.width();
            let quiet_zone = 2;
            let side_cells = width + 2 * quiet_zone;
            let cell_size = bounds.width / side_cells as f32;

            for y in 0..width {
                for x in 0..width {
                    if code[(x, y)] == qrcode::Color::Dark {
                        let cell_x = bounds.x + (x + quiet_zone) as f32 * cell_size;
                        let cell_y = bounds.y + (y + quiet_zone) as f32 * cell_size;
                        renderer.fill_quad(
                            cosmic::iced::advanced::renderer::Quad {
                                bounds: cosmic::iced::Rectangle::new(
                                    cosmic::iced::Point::new(cell_x, cell_y),
                                    cosmic::iced::Size::new(cell_size, cell_size),
                                ),
                                border: cosmic::iced::Border::default(),
                                shadow: cosmic::iced::Shadow::default(),
                                snap: false,
                            },
                            cosmic::iced::Color::BLACK,
                        );
                    }
                }
            }
        }
    }
}

impl<'a, Message> From<QrCodeWidget> for cosmic::Element<'a, Message>
where
    Message: 'a,
{
    fn from(widget: QrCodeWidget) -> Self {
        cosmic::Element::new(widget)
    }
}
