#[cfg(test)]
mod tests {
    use crate::events_source::{BuilderEvent, EventConsumer, EventsStreamer}; // EventsUpdater};
                                                                             //use crate::fetch::Fetch;
    use crate::events::{define_api, Error, Options};
    use async_compatibility_layer::art::async_spawn;
    use async_compatibility_layer::logging::{setup_backtrace, setup_logging};
    use async_std::stream::IntoStream;
    use async_std::sync::RwLock;
    use futures::stream::StreamExt;
    use hotshot_types::constants::{Version01, STATIC_VER_0_1};
    use hotshot_types::data::ViewNumber;
    use hotshot_types::event::{Event, EventType};
    use hotshot_types::traits::node_implementation::{ConsensusTime, NodeType};

    use hotshot_example_types::node_types::TestTypes;
    use std::sync::Arc;
    use std::time::Duration;
    use surf_disco::Client;
    use tide_disco::{App, Url};

    // return a empty transaction event
    fn generate_event<Types: NodeType<Time = ViewNumber>>(view_number: u64) -> Event<Types> {
        Event {
            view_number: ViewNumber::new(view_number),
            event: EventType::Transactions {
                transactions: vec![],
            },
        }
    }

    #[async_std::test]
    async fn test_event_stream() {
        tracing::info!("Starting hotshot test_event_stream");
        setup_logging();
        setup_backtrace();

        let port = portpicker::pick_unused_port().expect("Could not find an open port");
        let api_url = Url::parse(format!("http://localhost:{port}").as_str()).unwrap();

        let known_nodes_with_stake = vec![];
        let non_staked_node_count = 0;
        let events_streamer = Arc::new(RwLock::new(EventsStreamer::new(
            known_nodes_with_stake,
            non_staked_node_count,
        )));

        // Start the web server.
        let mut app = App::<_, Error, Version01>::with_state(events_streamer.clone());

        let hotshot_events_api =
            define_api::<Arc<RwLock<EventsStreamer<TestTypes>>>, TestTypes, Version01>(
                &Options::default(),
            )
            .expect("Failed to define hotshot eventsAPI");

        app.register_module("hotshot_events", hotshot_events_api)
            .expect("Failed to register hotshot events API");

        async_spawn(app.serve(api_url, STATIC_VER_0_1));

        // Start a client.
        let client = Client::<Error, Version01>::new(
            format!("http://localhost:{}/hotshot_events", port)
                .parse()
                .unwrap(),
        );
        assert!(client.connect(Some(Duration::from_secs(60))).await);

        tracing::info!("Connected to server");

        // client subscrive to hotshot events
        let mut events = client
            .socket("events")
            .subscribe::<BuilderEvent<TestTypes>>()
            .await
            .unwrap();

        tracing::info!("Subscribed to events");

        // Start a client.
        let client_2 = Client::<Error, Version01>::new(
            format!("http://localhost:{}/hotshot_events", port)
                .parse()
                .unwrap(),
        );
        assert!(client_2.connect(Some(Duration::from_secs(60))).await);

        tracing::info!("C2 Connected to server");

        // client subscrive to hotshot events
        let mut events_2 = client_2
            .socket("events")
            .subscribe::<BuilderEvent<TestTypes>>()
            .await
            .unwrap();

        tracing::info!("C2 Subscribed to events");

        let total_count = 5;
        //let stream = events.into_stream();
        // wait for these events to receive
        let receive_handle = async_spawn(async move {
            let mut receive_count = 0;
            loop {
                let event = events.next().await.unwrap();
                tracing::info!("Received event: {:?}", event);
                receive_count += 1;
                if receive_count >= total_count + 1 {
                    tracing::info!("Received all sent events, exiting loop");
                    break;
                }
            }
            // Offest 1 is due to the startup event info
            assert_eq!(receive_count, total_count + 1);
        });

        // wait for these events to receive
        let receive_handle_2 = async_spawn(async move {
            let mut receive_count = 0;
            loop {
                let event = events_2.next().await.unwrap();
                tracing::info!("Received event: {:?}", event);
                receive_count += 1;
                if receive_count >= total_count + 1 {
                    tracing::info!("Received all sent events, exiting loop");
                    break;
                }
            }
            // Offest 1 is due to the startup event info
            assert_eq!(receive_count, total_count + 1);
        });

        let send_handle = async_spawn(async move {
            let mut send_count = 0;
            loop {
                let tx_event = generate_event(send_count);
                tracing::debug!("Before writing to events_source");
                events_streamer
                    .write()
                    .await
                    .handle_event(tx_event.clone())
                    .await;
                send_count += 1;
                tracing::debug!("After writing to events_source");
                tracing::info!("Event sent: {:?}", tx_event);
                if send_count >= total_count {
                    break;
                }
            }
        });

        send_handle.await;
        receive_handle.await;
        receive_handle_2.await;
    }
}
