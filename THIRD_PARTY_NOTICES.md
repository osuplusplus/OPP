# Third-party notices

## osu-difficulty-icons

Difficulty icons are loaded at runtime from the static image URLs published by
[hiderikzki/osu-difficulty-icons](https://github.com/hiderikzki/osu-difficulty-icons).
No copies of those assets are bundled with OPP. The upstream collection is
licensed under GPL-3.0.

## osu-difficulty-lab runtime

The read-only osu!standard beatmap similarity runtime is derived from
`osuplusplus/osu-difficulty-lab` commit
`429352875ae4e0d7f44c45a64c4d604127b8c3b4`. The isolated osu!mania analyzer,
normalizer, data contract, and bucket similarity algorithm are derived from
commit `1fa21fa6a5144992df58efe7ce9d96019981fad3`.

Copyright (c) 2026 osuplusplus.

Both are used under the following MIT License. No dataset produced by that
project is included with OPP.

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

The upstream osu!mania implementation is described as an original clean-room
Rust implementation whose public design references
`LeoBlackMT/osumania_map_analyser` commit
`f146c479fde24523b4d0909b83f8008b9d6815b2`. OPP does not embed or execute its
JavaScript, generated models, WASM binaries, Roxy calibration head, or
MinaCalc. The referenced project is MIT-licensed; its notice and documentation
are available at
<https://github.com/LeoBlackMT/osumania_map_analyser/tree/f146c479fde24523b4d0909b83f8008b9d6815b2>.

## mania-converter-rust

OPP links the Rust crate `mania-converter` from
[Siflorite/mania-converter-rust](https://github.com/Siflorite/mania-converter-rust),
revision `5dcfc2205529485c060345b7f04df1c3bf9897ae`.

Copyright © Siflorite and contributors.

Licensed under the Apache License, Version 2.0. You may obtain a copy of the
License at <https://www.apache.org/licenses/LICENSE-2.0>. The upstream project
does not include a `NOTICE` file. OPP's own modifications remain MIT-licensed.
