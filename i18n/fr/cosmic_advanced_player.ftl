app-title = Cosmic Advanced Player
about = À propos
about-comments = Un beau lecteur de musique natif pour Linux, écrit en Rust avec libcosmic.
about-repository = Code source
about-issues = Signaler un problème

# Pages
page-now-playing = En lecture
page-songs = Morceaux
page-albums = Albums
page-artists = Artistes
page-genres = Genres
page-folders = Dossiers
page-favorites = Favoris
page-queue = File d’attente
page-search = Recherche
album = Album
artist = Artiste
back = Retour
unknown-album = Album inconnu
unknown-artist = Artiste inconnu

# Menus
menu-file = Fichier
menu-playback = Lecture
menu-view = Affichage
menu-help = Aide
open-folder = Ouvrir un dossier…
add-library-folder = Ajouter un dossier de musique…
rescan = Réindexer la bibliothèque
rescan-full = Réindexation complète
settings = Paramètres
quit = Quitter

# Transport
play = Lecture
pause = Pause
next = Suivant
previous = Précédent
stop = Arrêter
shuffle = Aléatoire
repeat = Répéter
repeat-off = Répétition : désactivée
repeat-all = Répétition : tout
repeat-one = Répétition : un morceau
seek-forward = Avancer de 10 s
seek-backward = Reculer de 10 s
volume-up = Monter le volume
volume-down = Baisser le volume
mute = Couper le son
unmute = Rétablir le son
nothing-playing = Aucune lecture
nothing-playing-body = Choisissez un morceau, un album ou ouvrez un dossier pour commencer.
up-next = À suivre : { $title } — { $artist }

# Track actions
play-next = Lire ensuite
add-to-queue = Ajouter à la file
go-to-album = Aller à l’album
go-to-artist = Aller à l’artiste
favorite = Ajouter aux favoris
unfavorite = Retirer des favoris
show-in-folder = Afficher dans le dossier
remove = Retirer

# Counts
track-count = { $count ->
    [one] 1 morceau
   *[other] { $count } morceaux
}
album-count = { $count ->
    [one] 1 album
   *[other] { $count } albums
}
artist-count = { $count ->
    [one] 1 artiste
   *[other] { $count } artistes
}
genre-count = { $count ->
    [one] 1 genre
   *[other] { $count } genres
}
artist-stats = { $albums ->
    [one] 1 album
   *[other] { $albums } albums
} · { $tracks ->
    [one] 1 morceau
   *[other] { $tracks } morceaux
}
library-stats = { $tracks } morceaux · { $albums } albums · { $duration } de musique
artist-unknown-body = Cet artiste n’est plus dans votre bibliothèque.

# Sorting
sort-artist = Artiste
sort-title = Titre
sort-album = Album
sort-duration = Durée
sort-added = Ajouts récents
sort-plays = Plus écoutés

# Library & scanning
welcome-title = Bienvenue !
welcome-body = Ajoutez les dossiers où vit votre musique : ils sont indexés récursivement et tenus à jour.
library-empty = Votre bibliothèque est vide
library-empty-body = Ajoutez un dossier de musique dans les paramètres, ou ouvrez un dossier pour l’écouter tout de suite.
no-library-folders = Aucun dossier de musique
no-library-folders-body = Ajoutez un dossier pour construire votre bibliothèque.
scanning = Indexation de votre musique…
scanning-progress = Indexation { $done } / { $total }
scan-done = Bibliothèque à jour : { $total } morceaux ({ $added } nouveaux, { $removed } retirés)
scan-failed = Échec de l’indexation : { $error }
watch-folders = Surveiller les dossiers
genres-empty = Aucun genre
genres-empty-body = Vos morceaux n’ont pas encore d’étiquette de genre.
favorites-empty = Pas encore de favoris
favorites-empty-body = Cliquez sur l’étoile d’un morceau pour le retrouver ici (et l’aimer sur Last.fm).

# Queue
queue-empty = La file d’attente est vide
queue-empty-body = Lancez un album ou ajoutez des morceaux avec « Ajouter à la file ».
queue-clear = Vider la suite
queue-stats = { $count } morceaux · { $remaining } restantes
queued = { $count ->
    [one] Ajouté à la file
   *[other] { $count } morceaux ajoutés à la file
}
queued-next = { $count ->
    [one] Sera lu ensuite
   *[other] { $count } morceaux seront lus ensuite
}
opened = { $count ->
    [one] Lecture d’un morceau
   *[other] Lecture de { $count } morceaux
}
nothing-to-play = Aucun fichier audio trouvé ici.
cannot-play = Impossible de lire « { $title } »
audio-device-error = Sortie audio indisponible : { $error }

# Search
search-placeholder = Morceaux, artistes, albums…
search-hint = Cherchez dans votre bibliothèque
search-hint-body = Tapez un titre, un artiste, un album ou un genre.
search-none = Aucun résultat
search-none-body = Rien ne correspond à « { $query } ».
search-results-for = Résultats pour « { $query } »

# Lyrics
lyrics = Paroles
lyrics-loading = Recherche des paroles…
lyrics-none = Aucune parole trouvée pour ce morceau.
lyrics-refresh = Chercher à nouveau
lyrics-source = { $source } · { $kind }
lyrics-synced = synchronisées
lyrics-plain = texte brut
lyrics-order-hint = Les sources sont essayées de haut en bas. Les paroles synchronisées sont privilégiées.
provider-local = Fichier .lrc ou .txt à côté du morceau
provider-embedded = Paroles enregistrées dans les tags du fichier
provider-lrclib = Paroles synchronisées de lrclib.net
provider-ovh = Paroles en texte brut de lyrics.ovh

# Settings
settings-library = Bibliothèque
settings-playback = Lecture
settings-lyrics = Plugins de paroles
settings-appearance = Apparence
replay-gain = Normalisation du volume (ReplayGain)
gapless = Lecture sans blanc (gapless)
theme = Thème
theme-system = Comme le bureau
theme-dark = Sombre
theme-light = Clair

# Last.fm
lastfm-help = Le scrobbling nécessite un compte API Last.fm (gratuit). Collez sa clé et son secret partagé, puis connectez-vous.
lastfm-create-key = Créer un compte API sur last.fm
lastfm-api-key = Clé d’API
lastfm-secret = Secret partagé
lastfm-connect = Se connecter à Last.fm
lastfm-connecting = Connexion…
lastfm-approve = Autorisez l’accès dans votre navigateur, puis revenez ici.
lastfm-finish = J’ai autorisé l’accès
lastfm-connected = Connecté à Last.fm en tant que { $user }
lastfm-connected-as = Connecté en tant que { $user }
lastfm-need-key = Saisissez d’abord votre clé d’API et votre secret Last.fm.
lastfm-error = Last.fm : { $error }
lastfm-expired = Votre session Last.fm a expiré, reconnectez-vous.
lastfm-scrobble = Scrobbler les morceaux écoutés
lastfm-now-playing = Mettre à jour « en cours d’écoute »
lastfm-love = Aimer les favoris sur Last.fm
lastfm-pending = { $count } scrobbles en attente d’envoi
disconnect = Se déconnecter

# Discord
discord-enable = Afficher ce que j’écoute sur Discord
discord-client-id = ID de l’application Discord
discord-client-id-help = Laissez vide pour utiliser l’application officielle « Cosmic Advanced Player ». Discord affiche le nom de l’application (« Écoute … »).
discord-create-app = Ouvrir le portail développeur Discord
discord-covers = Afficher les pochettes
discord-covers-help = Discord n’affiche que des images en ligne : les pochettes sont cherchées sur Deezer, iTunes et Last.fm.
discord-upload-covers = Partager les pochettes introuvables en ligne
discord-upload-covers-help = La pochette intégrée au fichier est envoyée sur un hébergeur temporaire (litterbox.catbox.moe ou uguu.se) qui la supprime après quelques heures.
