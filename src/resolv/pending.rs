//! A collection of pending request.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use futures::{Async, Future};
use rand;
use tokio_core::reactor::{Handle, Timeout};
use super::error::Error;
use super::request::ServiceRequest;


//------------ PendingRequests -----------------------------------------------

/// A collection of pending requests.
pub struct PendingRequests {
    /// A map from DNS message IDs to requests.
    ///
    /// If an ID is reserved, it maps to `None`, if a request has been
    /// pushed for it, it maps to `Some(_)`.
    requests: HashMap<u16, Option<ServiceRequest>>,

    /// An ordered list of message IDs and when they expire.
    ///
    /// Since we have a fixed duration and monotone time, we can use a
    /// simple deque here and push new requests to its end.
    expires: VecDeque<(u16, Instant)>,

    /// The optional future for the next time a request expires.
    timeout: Option<Timeout>,

    /// A handle to a reactor for creating timeout futures.
    reactor: Handle,

    /// The duration until a request expires.
    duration: Duration,
}

impl PendingRequests {
    /// Creates a new collection.
    pub fn new(reactor: Handle, expire: Duration) -> Self {
        PendingRequests {
            requests: HashMap::new(),
            expires: VecDeque::new(),
            timeout: None,
            reactor: reactor,
            duration: expire
        }
    }

    /// Returns whether there are no more pending requests.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Reserves a spot in the map and returns its ID.
    pub fn reserve(&mut self) -> Result<u16, ReserveError> {
        use std::collections::hash_map::Entry;

        // Pick a reasonably low number here so that we won’t hang too long
        // below.
        if self.requests.len() > 0xA000 {
            return Err(ReserveError);
        }
        // XXX I suppose there is a better way to do this. Anyone?
        loop {
            let id = rand::random();
            if let Entry::Vacant(entry) = self.requests.entry(id) {
                entry.insert(None);
                return Ok(id)
            }
        }
    }

    /// Drop a previously reserved id.
    pub fn unreserve(&mut self, id: u16) {
        match self.requests.remove(&id) {
            Some(Some(_)) => panic!("unreserving pushed ID"),
            Some(None) => { }
            None => panic!("unreserving unreserved ID")
        }
    }

    /// Adds the requests with the given ID to the collection.
    ///
    /// The `id` must have been reserved before and nothing been pushed
    /// for this ID since. Panics otherwise.
    pub fn push(&mut self, request: ServiceRequest) {
        let id = request.id();
        {
            let entry = self.requests.get_mut(&id)
                                     .expect("pushed unreserved ID");
            if entry.is_some() {
                panic!("pushed over existing ID");
            }
            *entry = Some(request);
        }
        self.expires.push_back((id, Instant::now() + self.duration));
        if self.timeout.is_none() {
            self.update_timeout();
        }
    }
    
    /// Removes and returns the request with the given ID.
    pub fn pop(&mut self, id: u16) -> Option<ServiceRequest> {
        if let Some(request) = self.requests.remove(&id) {
            if self.expires.front().unwrap().0 == id {
                self.expires.pop_front();
                self.update_timeout();
            }
            request
        }
        else { None }
    }

    /// Updates the timeout.
    ///
    /// Since we don’t delete the IDs in `pop()` (which could be expensive
    /// if they are somewhere in the middle), we need to pop items from the
    /// front until we find one that is actually still valid.
    fn update_timeout(&mut self) {
        while let Some(&(id, at)) = self.expires.front() {
            if self.requests.contains_key(&id) {
                // XXX What’s the best thing to do when getting a timeout
                //     fails?
                self.timeout = Timeout::new_at(at, &self.reactor).ok();
                return;
            }
            else {
                self.expires.pop_front();
            }
        }
        self.timeout = None
    }

    /// Removes and fails all expired requests.
    ///
    /// This method polls `self`’s timeout so it can only be called from
    /// within a task.
    pub fn expire(&mut self) {
        match self.timeout {
            Some(ref mut timeout) => {
                match timeout.poll() {
                    Ok(Async::NotReady) => return,
                    Ok(Async::Ready(())) => {
                        loop {
                            match self.expires.front() {
                                Some(&(_, at)) if at > Instant::now() => { }
                                _ => break
                            }
                            let id = self.expires.pop_front().unwrap().0;
                            if let Some(Some(item)) = self.requests
                                                          .remove(&id) {
                                item.fail(Error::Timeout)
                            }
                        }
                    }
                    Err(_) => {
                        // Fall through to update_timeout to perhaps fix
                        // the broken timeout.
                    }
                }
            }
            None => return
        }
        self.update_timeout();
        // Once more to register the timeout.
        self.expire()
    }
}


//--- Drop

impl Drop for PendingRequests {
    fn drop(&mut self) {
        for (_, item) in self.requests.drain() {
            if let Some(item) = item {
                item.fail(Error::Timeout)
            }
        }
    }
}


//------------ ReserveError --------------------------------------------------

/// An error happened while reserving an ID.
///
/// The only thing that can happen is that we run out of space.
pub struct ReserveError;

