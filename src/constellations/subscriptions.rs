use super::{Constellations, Message};
use crate::matrix;
use crate::utils::ipc;

use cosmic::iced::Subscription;
use matrix_sdk_ui::sync_service::State as SyncServiceState;
use std::sync::Arc;
use url::Url;

#[derive(Clone, Debug)]
pub(in crate::constellations) struct MatrixEngineWrapper(matrix::MatrixEngine);

impl std::hash::Hash for MatrixEngineWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "matrix-sync".hash(state);
    }
}

impl PartialEq for MatrixEngineWrapper {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for MatrixEngineWrapper {}

impl Constellations {
    pub(in crate::constellations) fn ipc_subscription(&self) -> Subscription<Message> {
        Subscription::run_with((), |_| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(async move {
                match ipc::start_server(tx).await {
                    Ok(_conn) => {
                        tracing::info!("IPC server started and waiting");
                    }
                    Err(e) => {
                        tracing::error!("Failed to start IPC server: {}", e);
                        return;
                    }
                }
                std::future::pending::<()>().await;
            });
            cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                loop {
                    if let Some(uri) = rx.recv().await {
                        if let Ok(url) = Url::parse(&uri) {
                            return Some((Message::OidcCallback(url), rx));
                        }
                    } else {
                        return None;
                    }
                }
            })
        })
    }

    pub(in crate::constellations) fn sync_subscription(
        &self,
        matrix: &matrix::MatrixEngine,
    ) -> Subscription<Message> {
        Subscription::run_with(MatrixEngineWrapper(matrix.clone()), |wrapper| {
            let engine = wrapper.0.clone();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            let tx_status = tx.clone();
            let engine_status = engine.clone();
            tokio::spawn(async move {
                let sync_service = loop {
                    if let Some(s) = engine_status.sync_service().await {
                        break s;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };

                let mut status_stream = sync_service.state();
                while let Some(status) = status_stream.next().await {
                    let sync_status = match status {
                            SyncServiceState::Idle => matrix::SyncStatus::Connected,
                            SyncServiceState::Running => matrix::SyncStatus::Syncing,
                            SyncServiceState::Terminated => matrix::SyncStatus::Disconnected,
                            SyncServiceState::Offline => matrix::SyncStatus::Disconnected,
                            SyncServiceState::Error(_) => {
                                matrix::SyncStatus::Error("Sync error encountered. This may be due to missing server support for Sliding Sync (MSC4186) or network issues.".to_string())
                            }
                        };
                    let _ = tx_status.send(Message::Matrix(
                        matrix::MatrixEvent::SyncStatusChanged(sync_status),
                    ));
                }
            });

            let tx_indicator = tx.clone();
            let engine_indicator = engine.clone();
            tokio::spawn(async move {
                let room_list_service = loop {
                    if let Some(rls) = engine_indicator.room_list_service().await {
                        break rls;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };

                let mut indicator_stream = Box::pin(room_list_service.sync_indicator(
                    std::time::Duration::from_millis(500),
                    std::time::Duration::from_millis(500),
                ));
                use cosmic::iced::futures::StreamExt;
                while let Some(indicator) = indicator_stream.next().await {
                    let show = indicator == matrix_sdk_ui::room_list_service::SyncIndicator::Show;
                    let _ = tx_indicator.send(Message::Matrix(
                        matrix::MatrixEvent::SyncIndicatorChanged(show),
                    ));
                }
            });

            let tx_ignored = tx.clone();
            let engine_ignored = engine.clone();
            tokio::spawn(async move {
                let client = engine_ignored.client().await;
                client.add_event_handler(
                    move |ev: matrix_sdk::ruma::events::ignored_user_list::IgnoredUserListEvent| {
                        let tx = tx_ignored.clone();
                        async move {
                            let users = ev.content.ignored_users.keys().cloned().collect();
                            let _ = tx.send(Message::Matrix(
                                matrix::MatrixEvent::IgnoredUsersChanged(users),
                            ));
                        }
                    },
                );
            });

            let tx_calls = tx.clone();
            let engine_calls = engine.clone();
            tokio::spawn(async move {
                let client = engine_calls.client().await;
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::call::member::CallMemberEventContent,
                    >,
                          room: matrix_sdk::Room| {
                        let tx = tx_calls.clone();
                        let engine = engine_calls.clone();
                        async move {
                            let room_id = room.room_id().to_string();
                            let participants = engine.get_call_participants(&room_id).await;
                            let _ = tx.send(Message::Matrix(
                                matrix::MatrixEvent::CallParticipantsChanged {
                                    room_id,
                                    participants,
                                },
                            ));
                        }
                    },
                );
            });

            let tx_hierarchy = tx.clone();
            let engine_hierarchy = engine.clone();
            tokio::spawn(async move {
                let client = engine_hierarchy.client().await;
                let tx_child = tx_hierarchy.clone();
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::space::child::SpaceChildEventContent,
                    >,
                          _room: matrix_sdk::Room| {
                        let tx = tx_child.clone();
                        async move {
                            let _ = tx
                                .send(Message::Matrix(matrix::MatrixEvent::SpaceHierarchyChanged));
                        }
                    },
                );
                let tx_parent = tx_hierarchy.clone();
                client.add_event_handler(
                    move |_ev: matrix_sdk::ruma::events::SyncStateEvent<
                        matrix_sdk::ruma::events::space::parent::SpaceParentEventContent,
                    >,
                          _room: matrix_sdk::Room| {
                        let tx = tx_parent.clone();
                        async move {
                            let _ = tx
                                .send(Message::Matrix(matrix::MatrixEvent::SpaceHierarchyChanged));
                        }
                    },
                );
            });

            let tx_rooms = tx.clone();
            let engine_rooms = engine.clone();
            tokio::spawn(async move {
                let room_list_service = loop {
                    if let Some(rls) = engine_rooms.room_list_service().await {
                        break rls;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                };
                let rooms = match room_list_service.all_rooms().await {
                    Ok(rooms) => rooms,
                    Err(_) => return,
                };
                let (stream, controller) = rooms.entries_with_dynamic_adapters(20);
                let controller = Arc::new(controller);
                engine_rooms
                    .set_room_list_controller(controller.clone())
                    .await;

                use matrix_sdk_ui::room_list_service::filters;
                controller.set_filter(Box::new(filters::new_filter_all(vec![])));

                use cosmic::iced::futures::StreamExt;
                let mut stream = Box::pin(stream);
                while let Some(diffs) = stream.next().await {
                    for diff in diffs {
                        let room_diff = match diff {
                            eyeball_im::VectorDiff::Insert { index, value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::Insert {
                                        index,
                                        value: data,
                                    })
                            }
                            eyeball_im::VectorDiff::Remove { index } => {
                                Some(eyeball_im::VectorDiff::Remove { index })
                            }
                            eyeball_im::VectorDiff::Set { index, value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::Set { index, value: data })
                            }
                            eyeball_im::VectorDiff::Reset { values } => {
                                let futures: Vec<_> = values
                                    .iter()
                                    .map(|v| get_room_data(&engine_rooms, v.room_id()))
                                    .collect();
                                let new_values: Vec<_> =
                                    cosmic::iced::futures::future::join_all(futures)
                                        .await
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                Some(eyeball_im::VectorDiff::Reset {
                                    values: new_values.into(),
                                })
                            }
                            eyeball_im::VectorDiff::Append { values } => {
                                let futures: Vec<_> = values
                                    .iter()
                                    .map(|v| get_room_data(&engine_rooms, v.room_id()))
                                    .collect();
                                let new_values: Vec<_> =
                                    cosmic::iced::futures::future::join_all(futures)
                                        .await
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                Some(eyeball_im::VectorDiff::Append {
                                    values: new_values.into(),
                                })
                            }
                            eyeball_im::VectorDiff::Truncate { length } => {
                                Some(eyeball_im::VectorDiff::Truncate { length })
                            }
                            eyeball_im::VectorDiff::PushBack { value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::PushBack { value: data })
                            }
                            eyeball_im::VectorDiff::PushFront { value } => {
                                get_room_data(&engine_rooms, value.room_id())
                                    .await
                                    .map(|data| eyeball_im::VectorDiff::PushFront { value: data })
                            }
                            eyeball_im::VectorDiff::PopBack => {
                                Some(eyeball_im::VectorDiff::PopBack)
                            }
                            eyeball_im::VectorDiff::PopFront => {
                                Some(eyeball_im::VectorDiff::PopFront)
                            }
                            eyeball_im::VectorDiff::Clear => Some(eyeball_im::VectorDiff::Clear),
                        };

                        if let Some(diff) = room_diff {
                            let _ = tx_rooms.send(Message::Matrix(matrix::MatrixEvent::RoomDiff(
                                Box::new(diff),
                            )));
                        }
                    }
                }
            });

            cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|msg| (msg, rx))
            })
        })
    }

    pub(in crate::constellations) fn timeline_subscription(
        &self,
        matrix: &matrix::MatrixEngine,
        room_id: Arc<str>,
    ) -> Subscription<Message> {
        Subscription::run_with(
            (MatrixEngineWrapper(matrix.clone()), room_id.clone()),
            |(wrapper, room_id)| {
                let engine = wrapper.0.clone();
                let room_id = room_id.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                tokio::spawn(async move {
                    let timeline = match engine.timeline(&room_id).await {
                        Ok(t) => t,
                        Err(_) => return,
                    };

                    let (items, mut stream) = timeline.subscribe().await;
                    let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineReset));

                    for (index, item) in items.into_iter().enumerate() {
                        let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineDiff(
                            eyeball_im::VectorDiff::Insert { index, value: item },
                        )));
                    }
                    let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineInitFinished));

                    use cosmic::iced::futures::StreamExt;
                    while let Some(diff) = stream.next().await {
                        for d in diff {
                            let _ = tx.send(Message::Matrix(matrix::MatrixEvent::TimelineDiff(d)));
                        }
                    }
                });

                cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                })
            },
        )
    }

    pub(in crate::constellations) fn threaded_timeline_subscription(
        &self,
        matrix: &matrix::MatrixEngine,
        room_id: Arc<str>,
        root_id: matrix_sdk::ruma::OwnedEventId,
    ) -> Subscription<Message> {
        Subscription::run_with(
            (
                MatrixEngineWrapper(matrix.clone()),
                room_id.clone(),
                root_id.clone(),
            ),
            |(wrapper, room_id, root_id)| {
                let engine = wrapper.0.clone();
                let room_id = room_id.clone();
                let root_id = root_id.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                tokio::spawn(async move {
                    let timeline = match engine.threaded_timeline(&room_id, &root_id).await {
                        Ok(t) => t,
                        Err(_) => return,
                    };

                    let (items, mut stream) = timeline.subscribe().await;
                    let _ = tx.send(Message::MatrixThreadReset(root_id.clone()));

                    for (index, item) in items.into_iter().enumerate() {
                        let _ = tx.send(Message::MatrixThreadDiff(
                            root_id.clone(),
                            eyeball_im::VectorDiff::Insert { index, value: item },
                        ));
                    }
                    let _ = tx.send(Message::MatrixThreadInitFinished(root_id.clone()));

                    use cosmic::iced::futures::StreamExt;
                    while let Some(diff) = stream.next().await {
                        for d in diff {
                            let _ = tx.send(Message::MatrixThreadDiff(root_id.clone(), d));
                        }
                    }
                });

                cosmic::iced::futures::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|msg| (msg, rx))
                })
            },
        )
    }
}

pub(in crate::constellations) async fn get_room_data(
    engine: &matrix::MatrixEngine,
    room_id: &matrix_sdk::ruma::RoomId,
) -> Option<matrix::RoomData> {
    let client = engine.client().await;
    let room = client.get_room(room_id)?;

    engine.fetch_room_data(&room).await.ok()
}
