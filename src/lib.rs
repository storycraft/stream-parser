/*
 * Created on Sat Jul 22 2023
 *
 * Copyright (c) storycraft. Licensed under the MIT Licence.
 */

#![doc = include_str!("../README.md")]

use std::{
    cell::Cell,
    fmt::Debug,
    fmt::{self, Formatter},
    io::{self, ErrorKind, Read},
    mem,
    pin::Pin,
    ptr::{self, NonNull},
    rc::Rc,
    task::{ready, Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use futures::{future::poll_fn, pin_mut, AsyncRead, AsyncWrite, Future, Stream, StreamExt};
use stream::DataStream;

use crate::adapter::AsyncAdapter;

pub(crate) mod adapter;
pub mod stream;

pub struct StreamParser<Item> {
    reader_slot: Rc<Cell<Option<NonNull<dyn AsyncRead>>>>,
    stream: Pin<Box<dyn Stream<Item = io::Result<Item>>>>,
}

impl<Item: 'static> StreamParser<Item> {
    pub fn new(
        read_fn: impl for<'b> IoReadFnMut<'b, DataStream<dyn AsyncRead>, Item> + 'static,
    ) -> Self {
        let reader_slot = Rc::new(Cell::new(None));

        let stream_reader_slot = reader_slot.clone();
        let stream = async_stream::stream!({
            let mut read_fn = read_fn;
            let stream = unsafe { DataStream::new(stream_reader_slot) };
            pin_mut!(stream);

            loop {
                yield read_fn.call(stream.as_mut()).await;
            }
        });

        Self {
            reader_slot,
            stream: Box::pin(stream),
        }
    }

    pub fn read(&mut self, reader: impl Read) -> io::Result<Item> {
        let reader = AsyncAdapter::new(reader);
        pin_mut!(reader);

        self.reader_slot.set(NonNull::new(unsafe {
            mem::transmute::<&mut dyn AsyncRead, &mut dyn AsyncRead>(&mut reader)
        }));

        let res = from_async_io(&mut self.stream, |stream, cx| {
            Poll::Ready(
                ready!(stream.poll_next(cx))
                    .unwrap_or_else(|| io::Result::Err(ErrorKind::UnexpectedEof.into())),
            )
        });
        self.reader_slot.take();

        res
    }

    pub fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        reader: impl AsyncRead,
    ) -> Poll<io::Result<Item>> {
        let reader = IoWithCx::new(reader, cx);
        pin_mut!(reader);

        self.reader_slot.set(NonNull::new(unsafe {
            mem::transmute::<Pin<&mut dyn AsyncRead>, &mut dyn AsyncRead>(reader)
        }));

        let waker = create_waker();
        let mut fake_cx = Context::from_waker(&waker);

        let res = self
            .stream
            .poll_next_unpin(&mut fake_cx)
            .map(|res| res.unwrap_or_else(|| io::Result::Err(ErrorKind::UnexpectedEof.into())));

        self.reader_slot.take();

        res
    }

    pub async fn read_async(&mut self, reader: impl AsyncRead) -> io::Result<Item> {
        let mut pinned = Pin::new(self);
        pin_mut!(reader);

        poll_fn(move |cx| pinned.as_mut().poll_read(cx, &mut reader)).await
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
pub(crate) struct IoWithCx<'a, 'cx, S> {
    #[pin]
    stream: S,
    cx: &'a mut Context<'cx>,
}

impl<'a, 'cx, S> IoWithCx<'a, 'cx, S> {
    pub fn new(stream: S, cx: &'a mut Context<'cx>) -> Self {
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

#[inline]
pub(crate) fn from_async_io<S: Unpin, T>(
    stream: &mut S,
    func: impl FnOnce(Pin<&mut S>, &mut Context) -> Poll<io::Result<T>>,
) -> io::Result<T> {
    let waker = create_waker();

    match func(Pin::new(stream), &mut Context::from_waker(&waker)) {
        Poll::Pending => Err(ErrorKind::WouldBlock.into()),
        Poll::Ready(res) => res,
    }
}
