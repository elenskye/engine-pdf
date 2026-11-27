"""生成可搜索副本用的占位字体 src/sys/ocr.ttf：空格和一个满格方块两个字形，文字靠 ToUnicode 交出。

只在要改字体时运行（需要 fontTools）：python3 scripts/ocr-font.py
"""

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

font = FontBuilder(1000, isTTF=True)
font.setupGlyphOrder([".notdef", "space", "box"])
font.setupCharacterMap({32: "space", 65: "box"})
pen = TTGlyphPen(None)
empty = pen.glyph()
pen.moveTo((0, -150))
pen.lineTo((1000, -150))
pen.lineTo((1000, 850))
pen.lineTo((0, 850))
pen.closePath()
font.setupGlyf({".notdef": empty, "space": empty, "box": pen.glyph()})
font.setupHorizontalMetrics({name: (1000, 0) for name in [".notdef", "space", "box"]})
font.setupHorizontalHeader(ascent=850, descent=-150)
font.setupNameTable({"familyName": "Engine PDF OCR", "styleName": "Regular", "psName": "EnginePDFOCR"})
font.setupOS2(sTypoAscender=850, sTypoDescender=-150, usWinAscent=850, usWinDescent=150)
font.setupPost()
font.save("src/sys/ocr.ttf")
