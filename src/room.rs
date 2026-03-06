use crate::utils::{Data, StateType};
use axum::extract::{
    ws::{Message, Utf8Bytes, WebSocket},
    State,
};
use futures_util::{SinkExt, StreamExt};

pub async fn interact(mut socket: WebSocket, mut state: State<StateType>) {
    let (mut sender, mut receiver) = socket.split();

    &state.lock().await.push(sender);

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(data)) => {
                match serde_json::from_str::<Data>(&data) {
                    Ok(parsed) => {
                        let send_data =
                            serde_json::to_string(&parsed).expect("there is error stringify!!!");
                        let utf8 = Utf8Bytes::from(send_data);
                        brodcast(Message::Text(utf8), &state).await;
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize: {:?}", e);
                    }
                };
            }
            Ok(Message::Binary(_)) => println!("Got binary"),
            Ok(Message::Ping(_)) => println!("Got ping"),
            Ok(Message::Pong(_)) => println!("Got pong"),
            Ok(Message::Close(_)) => println!("Got Close"),
            Err(e) => eprintln!("Somthing went wrong!!!"),
        }
    }
}

async fn brodcast(message: Message, state: &State<StateType>) {
    for so in state.clone().lock().await.iter_mut() {
        so.send(message.clone()).await.expect("TODO: panic message");
    }
}
