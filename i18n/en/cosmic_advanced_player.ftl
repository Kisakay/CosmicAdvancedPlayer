app-title = Cosmic Advanced Player
about = About
about-comments = A beautiful native music player for Linux, built with Rust and libcosmic.
about-repository = Source code
about-issues = Report an issue

# Pages
page-now-playing = Now playing
page-songs = Songs
page-albums = Albums
page-artists = Artists
page-genres = Genres
page-folders = Folders
page-favorites = Favorites
page-queue = Queue
page-search = Search
album = Album
artist = Artist
back = Back
unknown-album = Unknown album
unknown-artist = Unknown artist

# Menus
menu-file = File
menu-playback = Playback
menu-view = View
menu-help = Help
open-folder = Open folder…
add-library-folder = Add music folder…
rescan = Rescan library
rescan-full = Full rescan
settings = Settings
quit = Quit

# Transport
play = Play
pause = Pause
next = Next
previous = Previous
stop = Stop
shuffle = Shuffle
repeat = Repeat
repeat-off = Repeat: off
repeat-all = Repeat: all
repeat-one = Repeat: one track
seek-forward = Forward 10 s
seek-backward = Back 10 s
volume-up = Volume up
volume-down = Volume down
mute = Mute
unmute = Unmute
nothing-playing = Nothing playing
nothing-playing-body = Pick a song, an album or open a folder to start listening.
up-next = Up next: { $title } — { $artist }

# Track actions
play-next = Play next
add-to-queue = Add to queue
go-to-album = Go to album
go-to-artist = Go to artist
favorite = Add to favorites
unfavorite = Remove from favorites
show-in-folder = Show in folder
remove = Remove

# Counts
track-count = { $count ->
    [one] 1 track
   *[other] { $count } tracks
}
album-count = { $count ->
    [one] 1 album
   *[other] { $count } albums
}
artist-count = { $count ->
    [one] 1 artist
   *[other] { $count } artists
}
genre-count = { $count ->
    [one] 1 genre
   *[other] { $count } genres
}
artist-stats = { $albums ->
    [one] 1 album
   *[other] { $albums } albums
} · { $tracks ->
    [one] 1 track
   *[other] { $tracks } tracks
}
library-stats = { $tracks } tracks · { $albums } albums · { $duration } of music
artist-unknown-body = This artist is not in your library anymore.

# Sorting
sort-artist = Artist
sort-title = Title
sort-album = Album
sort-duration = Duration
sort-added = Recently added
sort-plays = Most played

# Library & scanning
welcome-title = Welcome!
welcome-body = Add the folders where your music lives: they are indexed recursively and kept up to date.
library-empty = Your library is empty
library-empty-body = Add a music folder in the settings, or open a folder to play it right away.
no-library-folders = No music folders
no-library-folders-body = Add a folder to build your library.
scanning = Indexing your music…
scanning-progress = Indexing { $done } / { $total }
scan-done = Library updated: { $total } tracks ({ $added } new, { $removed } removed)
scan-failed = Indexing failed: { $error }
watch-folders = Watch folders for changes
genres-empty = No genres
genres-empty-body = Your tracks have no genre tags yet.
favorites-empty = No favorites yet
favorites-empty-body = Click the star next to a song to keep it here (and love it on Last.fm).

# Queue
queue-empty = The queue is empty
queue-empty-body = Play an album or add songs with “Add to queue”.
queue-clear = Clear upcoming
queue-stats = { $count } tracks · { $remaining } remaining
queued = { $count ->
    [one] Added to the queue
   *[other] { $count } tracks added to the queue
}
queued-next = { $count ->
    [one] Will play next
   *[other] { $count } tracks will play next
}
opened = { $count ->
    [one] Playing 1 track
   *[other] Playing { $count } tracks
}
nothing-to-play = No audio files found there.
cannot-play = Cannot play “{ $title }”
audio-device-error = Audio output unavailable: { $error }

# Search
search-placeholder = Search songs, artists, albums…
search-hint = Search your library
search-hint-body = Type a title, an artist, an album or a genre.
search-none = No results
search-none-body = Nothing matches “{ $query }”.
search-results-for = Results for “{ $query }”

# Lyrics
lyrics = Lyrics
lyrics-loading = Looking for lyrics…
lyrics-none = No lyrics found for this song.
lyrics-refresh = Search again
lyrics-source = { $source } · { $kind }
lyrics-synced = synced
lyrics-plain = plain text
lyrics-order-hint = Sources are tried from top to bottom. Time-synced lyrics are preferred.
provider-local = .lrc or .txt file next to the song
provider-embedded = Lyrics stored in the file tags
provider-lrclib = Time-synced lyrics from lrclib.net
provider-ovh = Plain lyrics from lyrics.ovh

# Settings
settings-library = Library
settings-playback = Playback
settings-lyrics = Lyrics plugins
settings-appearance = Appearance
replay-gain = ReplayGain volume normalization
gapless = Gapless playback
theme = Theme
theme-system = Match desktop
theme-dark = Dark
theme-light = Light

# Last.fm
lastfm-help = Scrobbling needs a Last.fm API account (free). Paste its key and shared secret, then connect.
lastfm-create-key = Create an API account on last.fm
lastfm-api-key = API key
lastfm-secret = Shared secret
lastfm-connect = Connect to Last.fm
lastfm-connecting = Connecting…
lastfm-approve = Approve the access in your browser, then come back here.
lastfm-finish = I have approved it
lastfm-connected = Connected to Last.fm as { $user }
lastfm-connected-as = Connected as { $user }
lastfm-need-key = Enter your Last.fm API key and secret first.
lastfm-error = Last.fm: { $error }
lastfm-expired = Your Last.fm session expired, please reconnect.
lastfm-scrobble = Scrobble played tracks
lastfm-now-playing = Update “now playing”
lastfm-love = Love favorites on Last.fm
lastfm-pending = { $count } scrobbles waiting to be sent
disconnect = Disconnect

# Discord
discord-enable = Show what I listen to on Discord
discord-client-id = Discord application ID
discord-client-id-help = Leave empty to use the official “Cosmic Advanced Player” application. Discord shows the name of the application in “Listening to …”.
discord-create-app = Open the Discord developer portal
discord-covers = Show album covers
discord-covers-help = Covers are looked up online (Last.fm, iTunes) because Discord needs a public image URL.
