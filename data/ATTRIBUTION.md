# Data sources

`vi_syllables.tsv` and `en_words.tsv` are built by `cargo run -p ac-data --release` from:

- **Leipzig Corpora Collection**: `vie_news_2022_1M`, `vie-vn_web_2015_1M` word lists.
  D. Goldhahn, T. Eckart, U. Quasthoff (2012): *Building Large Monolingual Dictionaries at the Leipzig Corpora Collection: From 100 to 200 Languages*. LREC 2012. https://wortschatz.uni-leipzig.de (CC BY 4.0)
- **FrequencyWords** by Hermit Dave: `vi_50k.txt`, `en_50k.txt` (OpenSubtitles 2018). https://github.com/hermitdave/FrequencyWords (CC BY-SA 4.0)

Raw downloads live in `data/raw/` (not committed).

Vietnamese dialogue (word-pair and word-triple tables only; the text itself is not shipped):
OpenSubtitles2018 via OPUS, P. Lison and J. Tiedemann, 2016, "OpenSubtitles2016: Extracting Large Parallel Corpora from Movie and TV Subtitles" (LREC). https://opus.nlpl.eu/OpenSubtitles.php
