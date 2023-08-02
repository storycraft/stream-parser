/*
 * Created on Sat Jul 22 2023
 *
 * Copyright (c) storycraft. Licensed under the MIT Licence.
 */

#![doc = include_str!("../README.md")]

use std::{
    fmt::Debug,
    fmt::{self, Formatter},
    io::{self, ErrorKind, Read},
    mem::{self},
    pin::Pin,
    ptr::{self, NonNull},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use futures::{future::poll_fn, pin_mut, AsyncRead, AsyncWrite, Future, Stream, StreamExt};
use stream::DataStream;

use crate::adapter::AsyncAdapter;

pub(crate) mod adapter;
pub(crate) mod stream;

pub struct StreamParser<Item> {
    slot: Box<Option<NonNull<dyn AsyncRead>>>,
    stream: Pin<Box<dyn Stream<Item = io::Result<Item>>>>,
}

impl<Item: 'static> StreamParser<Item> {
    pub fn new(
        read_fn: impl for<'b> IoReadFnMut<'b, DataStream<dyn AsyncRead>, Item> + 'static,
    ) -> Self {
        let mut slot = Box::new(None);

        let slot_ptr = NonNull::from(&mut *slot);
        let stream = async_stream::stream!({
            let mut read_fn = read_fn;
            let stream = unsafe { DataStream::new(slot_ptr) };
            pin_mut!(stream);

            loop {
                yield read_fn.call(stream.as_mut()).await;
            }
        });

        Self {
            slot,
            stream: Box::pin(stream),
        }
    }

    fn poll_stream_with(&mut self, reader: Pin<&mut dyn AsyncRead>) -> Poll<io::Result<Item>> {
        let slot = &mut *self.slot;
        *slot = Some(NonNull::from(unsafe {
            mem::transmute::<Pin<&mut dyn AsyncRead>, &mut dyn AsyncRead>(reader)
        }));
        scopeguard::defer!({
            slot.take();
        });

        let waker = create_waker();
        let mut fake_cx = Context::from_waker(&waker);

        self.stream
            .poll_next_unpin(&mut fake_cx)
            .map(|res| res.unwrap_or_else(|| io::Result::Err(ErrorKind::UnexpectedEof.into())))
    }

    pub fn read(&mut self, reader: impl Read) -> io::Result<Item> {
        let reader = AsyncAdapter::new(reader);
        pin_mut!(reader);

        match self.poll_stream_with(reader) {
            Poll::Ready(item) => item,
            Poll::Pending => Err(ErrorKind::WouldBlock.into()),
        }
    }

    pub fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        reader: impl AsyncRead,
    ) -> Poll<io::Result<Item>> {
        let reader = IoWithCx::new(reader, cx);
        pin_mut!(reader);

        self.poll_stream_with(reader)
    }

    pub async fn read_async(&mut self, reader: impl AsyncRead) -> io::Result<Item> {
        pin_mut!(reader);

        poll_fn(move |cx| self.poll_read(cx, reader.as_mut())).await
    }
}

impl<Item> Debug for StreamParser<Item> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamParser").finish_non_exhaustive()
    }
}

unsafe impl<Item> Send for StreamParser<Item> {}
unsafe impl<Item> Sync for StreamParser<Item> {}

pub trait IoReadFnMut<'a, R, T> {
    type Fut: Future<Output = io::Result<T>> + 'a;
    fn call(&mut self, reader: Pin<&'a mut R>) -> Self::Fut;
}

impl<'a, F: FnMut(Pin<&'a mut R>) -> Fut, Fut, R, T> IoReadFnMut<'a, R, T> for F
where
    Fut: Future<Output = io::Result<T>> + 'a,
    &'a mut R: 'a,
{
    type Fut = Fut;

    fn call(&mut self, reader: Pin<&'a mut R>) -> Self::Fut {
        self(reader)
    }
}

#[derive(Debug)]
#[pin_project::pin_project]
pub(crate) struct IoWithCx<'a, 'waker, S> {
    #[pin]
    stream: S,
    cx: &'a mut Context<'waker>,
}

impl<'a, 'waker, S> IoWithCx<'a, 'waker, S> {
    pub fn new(stream: S, cx: &'a mut Context<'waker>) -> Self {
        Self { stream, cx }
    }
}

impl<S: AsyncRead> AsyncRead for IoWithCx<'_, '_, S> {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();

        this.stream.poll_read(this.cx, buf)
    }
}

impl<S: AsyncWrite> AsyncWrite for IoWithCx<'_, '_, S> {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.project();

        this.stream.poll_write(this.cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<io::Result<()>> {
        let this = self.project();

        this.stream.poll_flush(this.cx)
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context) -> Poll<io::Result<()>> {
        let this = self.project();

        this.stream.poll_close(this.cx)
    }
}

pub(crate) fn create_waker() -> Waker {
    const NO_OP_RAW_WAKER: RawWaker = {
        fn wake(_: *const ()) {
            unreachable!("Io task must not be woken by io waker")
        }

        RawWaker::new(
            ptr::null(),
            &RawWakerVTable::new(|_| NO_OP_RAW_WAKER, wake, wake, |_| {}),
        )
    };

    unsafe { Waker::from_raw(NO_OP_RAW_WAKER) }
}
