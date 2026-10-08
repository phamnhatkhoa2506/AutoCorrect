# Data sources

`vi_syllables.tsv` and `en_words.tsv` are built by `cargo run -p ac-data --release` from:

- **Leipzig Corpora Collection**: `vie_news_2022_1M`, `vie-vn_web_2015_1M` word lists.
  D. Goldhahn, T. Eckart, U. Quasthoff (2012): *Building Large Monolingual Dictionaries at the Leipzig Corpora Collection: From 100 to 200 Languages*. LREC 2012. https://wortschatz.uni-leipzig.de (CC BY 4.0)
- **FrequencyWords** by Hermit Dave: `vi_50k.txt`, `en_50k.txt` (OpenSubtitles 2018). https://github.com/hermitdave/FrequencyWords (CC BY-SA 4.0)

Raw downloads live in `data/raw/` (not committed).

Vietnamese dialogue (word-pair and word-triple tables only; the text itself is not shipped):
OpenSubtitles2018 via OPUS, P. Lison and J. Tiedemann, 2016, "OpenSubtitles2016: Extracting Large Parallel Corpora from Movie and TV Subtitles" (LREC). https://opus.nlpl.eu/OpenSubtitles.php

## Viwiki-Spelling (evaluation only)

`data/raw/viwiki_spelling/spelling_test.json` (not committed: `data/raw/` is ignored): 107 Vietnamese Wikipedia documents with annotated spelling mistakes, from https://github.com/heraclex12/Viwiki-spelling. Licence: CC BY 4.0. Creators: Hieu Tran, Cuong V. Dinh, Long Phan, Son T. Nguyen, "Hierarchical Transformer Encoders for Vietnamese Spelling Correction" (arXiv 2105.13578). Used only as an outside yardstick for measuring the corrector, never to build the word tables.

## VSEC (real misspellings; for evaluation and calibrating the typist model)

`data/raw/vsec/VSEC.jsonl` (not committed: `data/raw/` is ignored): 9341 Vietnamese sentences with 11202 manually corrected misspellings, from https://github.com/VSEC2021/VSEC. The repository states no licence and no paper; personal research use only, not redistributed.
