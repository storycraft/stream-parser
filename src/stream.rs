/*
 * Created on Mon Jul 24 2023
 *
 * Copyright (c) storycraft. Licensed under the MIT Licence.
 */

use std::{
    io,
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};

#[derive(Debug)]
#[pin_project::pin_project]
pub struct DataStream<S: ?Sized>(NonNull<Option<NonNull<S>>>);

impl<S: ?Sized> DataStream<S> {
    pub(crate) const unsafe fn new<'a>(ptr: NonNull<Option<NonNull<S>>>) -> Self
    where
        Self: 'a,
    {
        Self(ptr)
    }

    fn get_stream(self: Pin<&mut Self>) -> Pin<&mut S> {
        unsafe { Pin::new_unchecked((&mut *self.0.as_ptr()).expect("Stream slot is not set").as_mut()) }
    }
}

impl<S: ?Sized + AsyncRead> AsyncRead for DataStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.get_stream().poll_read(cx, buf)
    }
}

impl<S: ?Sized + AsyncWrite> AsyncWrite for DataStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_stream().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_stream().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_stream().poll_close(cx)
    }
}
