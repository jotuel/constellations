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

        let (homeserver_elem, username_elem, password_elem) = self.login_inputs();
        content = content
            .push(homeserver_elem)
            .push(username_elem)
            .push(password_elem);

        let is_missing_fields = self.login_homeserver.trim().is_empty()
            || self.login_username.trim().is_empty()
            || self.login_password.is_empty();

        content = content.push(self.login_main_button(is_missing_fields));

        if !self.is_registering_mode {
            content = content.push(self.login_oidc_button());
            content = content.push(self.login_qr_button());
        }

        content = content.push(self.login_toggle_button());

        container(content)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    fn login_inputs(
        &self,
    ) -> (
        Element<'_, Message>,
        Element<'_, Message>,
        Element<'_, Message>,
    ) {
        let homeserver_input = text_input(crate::fl!("homeserver"), &self.login_homeserver);
        let username_input = text_input(crate::fl!("username"), &self.login_username);
        let password_input = text_input(crate::fl!("password"), &self.login_password).password();

        if (self.auth_flow == AuthFlow::Password || self.auth_flow == AuthFlow::Oidc)
            || self.is_registering
        {
            (
                homeserver_input.into(),
                username_input.into(),
                password_input.into(),
            )
        } else {
            (
                homeserver_input
                    .on_input(Message::LoginHomeserverChanged)
                    .into(),
                username_input
                    .on_input(Message::LoginUsernameChanged)
                    .into(),
                password_input
                    .on_input(Message::LoginPasswordChanged)
                    .on_submit(|_| {
                        if self.is_registering_mode {
                            Message::SubmitRegister
                        } else {
                            Message::SubmitLogin
                        }
                    })
                    .into(),
            )
        }
    }

    fn login_main_button(&self, is_missing_fields: bool) -> Element<'_, Message> {
        if self.is_registering_mode {
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
        }
    }

    fn login_oidc_button(&self) -> Element<'_, Message> {
        if self.auth_flow == AuthFlow::Oidc {
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
        }
    }

    fn login_qr_button(&self) -> Element<'_, Message> {
        let mut btn = button::text(crate::fl!("login-qr-button"));
        if self.auth_flow != AuthFlow::Password
            && !self.is_registering_mode
            && self.auth_flow != AuthFlow::Oidc
        {
            btn = btn.on_press(Message::StartQrLogin);
        }
        btn.into()
    }

    fn login_toggle_button(&self) -> Element<'_, Message> {
        let toggle_mode_button = if self.is_registering_mode {
            button::text(crate::fl!("already-have-account"))
        } else {
            button::text(crate::fl!("need-account"))
        };

        if (self.auth_flow == AuthFlow::Password || self.auth_flow == AuthFlow::Oidc)
            || self.is_registering
        {
            toggle_mode_button.into()
        } else {
            toggle_mode_button.on_press(Message::ToggleLoginMode).into()
        }
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

                if let Some(ref bytes) = self.qr_code_bytes {
                    content = content.push(container(QrCodeWidget::new(bytes.clone())).padding(15));
                }
            }
            QrLoginStep::AwaitingCheckCode => {
                content = content.push(text::body(crate::fl!("login-qr-check-code-prompt")));
                let input = text_input(
                    crate::fl!("login-qr-check-code-placeholder"),
                    &self.qr_check_code_input,
                )
                .on_input(Message::QrCheckCodeChanged)
                .on_submit(|_| Message::SubmitQrCheckCode);
                content = content.push(input);
            }
            QrLoginStep::Authenticating => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-authenticating")));
            }
            QrLoginStep::SyncingSecrets => {
                content = content
                    .push(container(
                        cosmic::widget::progress_bar::indeterminate_circular().size(32.0),
                    ))
                    .push(text::body(crate::fl!("login-qr-syncing")));
            }
            QrLoginStep::Success => {
                content = content.push(text::body(crate::fl!("login-qr-success")));
            }
            QrLoginStep::Error => {
                content = content.push(text::body(crate::fl!("qr-login-error")));
            }
            QrLoginStep::NotStarted => {
                content = content.push(text::body(crate::fl!("qr-login-error")));
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
    bytes: Vec<u8>,
}

impl QrCodeWidget {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
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

        if let Ok(code) = qrcode::QrCode::new(&self.bytes) {
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
