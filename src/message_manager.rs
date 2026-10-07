use std::{
    io::{self, Read, Write},
    num::NonZeroU32,
    time::Instant,
};

use protobuf::Message;

use crate::{
    cast::{
        cast_channel,
        cast_channel::cast_message::{PayloadType, ProtocolVersion},
    },
    errors::Error,
    utils, Lock,
};

/// Type of the payload that `CastMessage` can have.
#[derive(Debug, Clone)]
pub enum CastMessagePayload {
    /// Payload represented by UTF-8 string (usually it's just a JSON string).
    String(String),
    /// Payload represented by binary data.
    Binary(Vec<u8>),
}

/// Base structure that represents messages that are exchanged between Receiver and Sender.
#[derive(Debug, Clone)]
pub struct CastMessage {
    /// A namespace is a labeled protocol. That is, messages that are exchanged throughout the
    /// Cast ecosystem utilize namespaces to identify the protocol of the message being sent.
    pub namespace: String,
    /// Unique identifier of the `sender` application.
    pub source: String,
    /// Unique identifier of the `receiver` application.
    pub destination: String,
    /// Payload data attached to the message (either string or binary).
    pub payload: CastMessagePayload,
}

/// Observes every message read from the stream, see [`MessageManager::set_observer`]. It is
/// implemented for closures taking a `&CastMessage`.
pub trait Observer: Send {
    fn on_message(&self, message: &CastMessage);
}

impl<F: Fn(&CastMessage) + Send> Observer for F {
    fn on_message(&self, message: &CastMessage) {
        self(message)
    }
}

/// Static structure that is responsible for (de)serializing and sending/receiving Cast protocol
/// messages.
pub struct MessageManager<S>
where
    S: Write + Read,
{
    message_buffer: Lock<Vec<CastMessage>>,
    stream: Lock<S>,
    request_counter: Lock<NonZeroU32>,
    deadline: Lock<Option<Instant>>,
    answer_pings: Lock<bool>,
    observer: Lock<Option<Box<dyn Observer>>>,
}

impl<S> MessageManager<S>
where
    S: Write + Read,
{
    pub fn new(stream: S) -> Self {
        MessageManager {
            stream: Lock::new(stream),
            message_buffer: Lock::new(vec![]),
            request_counter: Lock::new(NonZeroU32::MIN),
            deadline: Lock::new(None),
            answer_pings: Lock::new(false),
            observer: Lock::new(None),
        }
    }

    /// Sets whether heartbeat pings received while waiting for messages with `receive_find_map`
    /// are automatically replied to (the default is not to). Cast devices close connections whose
    /// pings go unanswered for a few seconds, so this allows waiting for longer than that.
    pub fn set_answer_pings(&self, answer_pings: bool) {
        *self.answer_pings.borrow_mut() = answer_pings;
    }

    /// Sets a deadline for waiting for messages with `receive_find_map`, which is used by all the
    /// requests waiting for a reply. Once it has passed, waiting fails with an `Error::Io` of kind
    /// `TimedOut`. `None` (the default) waits indefinitely.
    ///
    /// The deadline is checked whenever a message is received. Cast devices regularly send
    /// heartbeat pings, so waiting stops shortly after the deadline even if nothing else is
    /// received. Unlike a read timeout, the connection can still be used afterwards.
    pub fn set_deadline(&self, deadline: Option<Instant>) {
        *self.deadline.borrow_mut() = deadline;
    }

    /// Sets an observer of every message read from the stream, replacing any previous one. It is
    /// given the messages in the order they are received, before anything else is done with them
    /// (e.g. matching them in `receive_find_map`, or buffering them). This allows keeping track of
    /// messages nobody is waiting for, such as status broadcasts. Messages returned from the
    /// internal buffer are not observed again.
    ///
    /// The observer must not use this `MessageManager`.
    pub fn set_observer(&self, observer: impl Observer + 'static) {
        *self.observer.borrow_mut() = Some(Box::new(observer));
    }

    /// Sends `message` to the Cast Device.
    ///
    /// # Arguments
    ///
    /// * `message` - `CastMessage` instance to be sent to the Cast Device.
    pub fn send(&self, message: CastMessage) -> Result<(), Error> {
        let mut raw_message = cast_channel::CastMessage::new();

        raw_message.set_protocol_version(ProtocolVersion::CASTV2_1_0);

        raw_message.set_namespace(message.namespace);
        raw_message.set_source_id(message.source);
        raw_message.set_destination_id(message.destination);

        match message.payload {
            CastMessagePayload::String(payload) => {
                raw_message.set_payload_type(PayloadType::STRING);
                raw_message.set_payload_utf8(payload);
            }

            CastMessagePayload::Binary(payload) => {
                raw_message.set_payload_type(PayloadType::BINARY);
                raw_message.set_payload_binary(payload);
            }
        };

        let frame = utils::to_frame(&raw_message)?;

        self.stream.borrow_mut().write_all(&frame)?;

        log::debug!("Message sent: {:?}", raw_message);

        Ok(())
    }

    /// Waits for the next `CastMessage` available. Can also return existing message from the
    /// internal message buffer containing messages that have been received previously, but haven't
    /// been consumed for some reason (e.g. during `receive_find_map` call).
    ///
    /// # Return value
    ///
    /// `Result` containing parsed `CastMessage` or `Error`.
    pub fn receive(&self) -> Result<CastMessage, Error> {
        let mut message_buffer = self.message_buffer.borrow_mut();

        // If we have messages in the buffer, let's return them from it.
        if message_buffer.is_empty() {
            self.read()
        } else {
            Ok(message_buffer.remove(0))
        }
    }

    /// Waits for the next `CastMessage` for which `f` returns valid mapped value. Messages in which
    /// `f` is not interested are placed into internal message buffer and can be later retrieved
    /// with `receive`. This method always reads from the stream.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::net::TcpStream;
    /// # use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
    /// # use rust_cast::message_manager::{CastMessage, MessageManager};
    /// # let connector = SslConnector::builder(SslMethod::tls()).unwrap().build();
    /// # let tcp_stream = TcpStream::connect(("0", 8009)).unwrap();
    /// # let ssl_stream = connector.connect("0", tcp_stream).unwrap();
    /// # let message_manager = MessageManager::new(ssl_stream);
    /// # fn can_handle(message: &CastMessage) -> bool { unimplemented!() }
    /// # fn parse(message: &CastMessage) { unimplemented!() }
    /// message_manager.receive_find_map(|message| {
    ///   if !can_handle(message) {
    ///     return Ok(None);
    ///   }
    ///
    ///   parse(message);
    ///
    ///   Ok(Some(()))
    /// })?;
    /// # Ok::<(), rust_cast::errors::Error>(())
    /// ```
    ///
    /// # Arguments
    ///
    /// * `f` - Function that analyzes and maps `CastMessage` to any other type. If message doesn't
    /// look like something `f` is looking for, then `Ok(None)` should be returned so that message
    /// is not lost and placed into internal message buffer for later retrieval.
    ///
    /// # Return value
    ///
    /// `Result` containing parsed `CastMessage` or `Error`.
    pub fn receive_find_map<F, B>(&self, f: F) -> Result<B, Error>
    where
        F: Fn(&CastMessage) -> Result<Option<B>, Error>,
    {
        loop {
            let message = self.read()?;

            if *self.answer_pings.borrow() && is_ping(&message) {
                self.send(CastMessage {
                    namespace: message.namespace,
                    source: message.destination,
                    destination: message.source,
                    payload: CastMessagePayload::String(r#"{"type":"PONG"}"#.to_string()),
                })?;
            } else {
                // If message is found, just return mapped result, otherwise keep unprocessed
                // message in the buffer, it can be later retrieved with `receive`.
                match f(&message)? {
                    Some(r) => return Ok(r),
                    None => self.message_buffer.borrow_mut().push(message),
                }
            }

            if matches!(*self.deadline.borrow(), Some(deadline) if Instant::now() >= deadline) {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline exceeded while waiting for a message",
                )));
            }
        }
    }

    /// Generates unique integer number that is used in some requests to map them with the response.
    ///
    /// # Return value
    ///
    /// Unique (in the scope of this particular `MessageManager` instance) integer number.
    pub fn generate_request_id(&self) -> NonZeroU32 {
        let mut counter = self.request_counter.borrow_mut();
        let request_id = *counter;
        *counter = counter.checked_add(1).unwrap();
        request_id
    }

    /// Drains the internal message buffer, returning the messages it contained (oldest first).
    pub fn drain(&self) -> Vec<CastMessage> {
        std::mem::take(&mut *self.message_buffer.borrow_mut())
    }

    /// Calls `f` with a reference to the underlying stream, e.g. to configure socket options.
    pub(crate) fn with_stream<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        f(&self.stream.borrow())
    }

    /// Reads next `CastMessage` from the stream, passing it to the observer (if any).
    ///
    /// # Return value
    ///
    /// `Result` containing parsed `CastMessage` or `Error`.
    fn read(&self) -> Result<CastMessage, Error> {
        let message = self.read_from_stream()?;

        if let Some(observer) = &*self.observer.borrow() {
            observer.on_message(&message);
        }

        Ok(message)
    }

    fn read_from_stream(&self) -> Result<CastMessage, Error> {
        let buffer = {
            let reader = &mut *self.stream.borrow_mut();

            let mut length = [0; 4];
            reader.read_exact(&mut length)?;

            let mut buffer = vec![0; u32::from_be_bytes(length) as usize];
            reader.read_exact(&mut buffer)?;
            buffer
        };

        let mut raw_message = cast_channel::CastMessage::parse_from_bytes(&buffer)?;

        log::debug!("Message received: {:?}", raw_message);

        Ok(CastMessage {
            namespace: raw_message.take_namespace(),
            source: raw_message.take_source_id(),
            destination: raw_message.take_destination_id(),
            payload: match raw_message.payload_type() {
                PayloadType::STRING => CastMessagePayload::String(raw_message.take_payload_utf8()),
                PayloadType::BINARY => {
                    CastMessagePayload::Binary(raw_message.take_payload_binary())
                }
            },
        })
    }
}

const HEARTBEAT_NAMESPACE: &str = "urn:x-cast:com.google.cast.tp.heartbeat";

fn is_ping(message: &CastMessage) -> bool {
    match message.payload {
        CastMessagePayload::String(ref payload) if message.namespace == HEARTBEAT_NAMESPACE => {
            serde_json::from_str::<serde_json::Value>(payload)
                .is_ok_and(|payload| payload["type"] == "PING")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{Arc, Mutex},
    };

    use super::*;

    fn message(namespace: &str) -> CastMessage {
        CastMessage {
            namespace: namespace.to_string(),
            source: "receiver-0".to_string(),
            destination: "sender-0".to_string(),
            payload: CastMessagePayload::String("{}".to_string()),
        }
    }

    #[test]
    fn observer_sees_every_message_once_in_arrival_order() {
        // frame the messages to receive with `send`.
        let writer = MessageManager::new(Cursor::new(Vec::new()));
        for namespace in ["a", "b", "c"] {
            writer.send(message(namespace)).unwrap();
        }
        let stream = writer.stream.borrow().get_ref().clone();

        let observed = Arc::new(Mutex::new(Vec::new()));
        let manager = MessageManager::new(Cursor::new(stream));
        manager.set_observer({
            let observed = observed.clone();
            move |message: &CastMessage| observed.lock().unwrap().push(message.namespace.clone())
        });

        // "a" is buffered while looking for "b", then returned from the buffer.
        manager
            .receive_find_map(|message| Ok((message.namespace == "b").then_some(())))
            .unwrap();
        assert_eq!(manager.receive().unwrap().namespace, "a");
        assert_eq!(manager.receive().unwrap().namespace, "c");

        assert_eq!(*observed.lock().unwrap(), ["a", "b", "c"]);
    }
}
