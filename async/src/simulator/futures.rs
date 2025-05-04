use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use futures::future::FusedFuture;
use pin_project_lite::pin_project;

pin_project! {
    #[derive(Debug, Copy, Clone)]
    pub struct Sleep {
        #[pin]
        now: SystemTime,
        #[pin]
        duration: Duration,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Sleep {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self {
            now: dst_demo_time::now(),
            duration,
            polled: false,
            completed: false,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        log::trace!(
            "Polling Sleep: now={:?} duration={:?} polled={} completed={}",
            this.now,
            this.duration,
            this.polled,
            this.completed,
        );
        if !*this.polled {
            log::debug!("polling sleep for the first time");
            *this.polled.as_mut() = true;
            return Poll::Pending;
        }
        if dst_demo_time::now().duration_since(*this.now).unwrap() >= *this.duration {
            *this.completed.as_mut() = true;
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl FusedFuture for Sleep {
    fn is_terminated(&self) -> bool {
        self.completed
    }
}
