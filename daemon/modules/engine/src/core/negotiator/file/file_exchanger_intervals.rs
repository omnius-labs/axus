use std::time::Duration;

/// The cycles and deadlines of FileExchanger. The defaults are the values of the design (§5.8),
/// and tests replace them with short ones. The peer is not told these values, so they only affect the local timing.
#[derive(Debug, Clone)]
pub struct FileExchangerIntervals {
    /// How often a status message is sent even when nothing changed.
    pub status_cycle: Duration,
    /// How long a request may wait for a block or a do-not-have response since the last response.
    pub request_timeout: Duration,
    /// How long a Session neither side wants is kept.
    pub unwanted_close: Duration,
    /// How long a pair closed because neither side wanted is kept out of the dialing candidates.
    pub unwanted_cooldown: Duration,
    /// How long a pair rejected by the peer is kept out of the dialing candidates.
    pub reject_retry: Duration,
    /// How long a node that answered a rank with no present bit is not asked for that rank.
    pub rank_absent_retry: Duration,
    /// The period of the received bytes that ranks the nodes to unchoke.
    pub ranking_window: Duration,
    /// How often the node unchoked at random is chosen again.
    pub unchoke_rotation: Duration,
    /// How long a node that violated the protocol is excluded.
    pub violation_exclusion: Duration,
    /// How long a node that exceeded the request or receive deadline is excluded.
    pub timeout_exclusion: Duration,
    /// How often a Session worker checks what to send and request.
    pub poll: Duration,
    /// How often the connection task looks for peers to dial.
    pub connect: Duration,
    /// How long the blocks to fetch are reused before they are read from the Store again.
    pub wanted_refresh: Duration,
}

impl FileExchangerIntervals {
    /// A Session that receives no message for this long is closed.
    pub fn receive_timeout(&self) -> Duration {
        self.status_cycle * 3
    }
}

impl Default for FileExchangerIntervals {
    fn default() -> Self {
        Self {
            status_cycle: Duration::from_secs(10),
            request_timeout: Duration::from_secs(60),
            unwanted_close: Duration::from_secs(60),
            unwanted_cooldown: Duration::from_secs(10 * 60),
            reject_retry: Duration::from_secs(30),
            rank_absent_retry: Duration::from_secs(30),
            ranking_window: Duration::from_secs(10 * 60),
            unchoke_rotation: Duration::from_secs(30),
            violation_exclusion: Duration::from_secs(30 * 60),
            timeout_exclusion: Duration::from_secs(3 * 60),
            poll: Duration::from_millis(200),
            connect: Duration::from_secs(1),
            wanted_refresh: Duration::from_millis(500),
        }
    }
}
