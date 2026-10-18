"""Обучает дешёвый фильтр перед LLM: «есть в сообщении что-то полезное или это болтовня».

Запуск:  pip3 install scikit-learn   (один раз)
         python3 -m eduhub_agent.llm.train_classifier
Берёт chats/dataset.jsonl (build_dataset.py) и разметку chat_labels.txt,
печатает сравнение с ключевыми словами и сохраняет chats/filter_model.pkl.
Сообщения с файлами в агенте всё равно идут в модель по правилу.
"""
import json
import pickle

import numpy as np
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import StratifiedKFold
from sklearn.pipeline import make_pipeline, make_union

from eduhub_agent.paths import CHATS, data

TARGET_RECALL = 0.95  # какую долю полезных сообщений фильтр обязан пропустить


def load():
    rows = [json.loads(l) for l in open(CHATS / "dataset.jsonl", encoding="utf-8")]
    codes = {}
    for line in open(data("chat_labels.txt"), encoding="utf-8"):
        if line.startswith("#"):
            continue
        for tok in line.split():
            codes[int(tok.rstrip("htlmip"))] = tok.lstrip("0123456789")
    for i, r in enumerate(rows):
        r["code"] = codes.get(i, "n")
    return [r for r in rows if r["code"] != "p"]


def text(r):
    return r["text"].lower() + (" ⟨файл⟩" if r["files"] else "") + (" ⟨ссылка⟩" if r["links"] else "")


def model():
    return make_pipeline(
        make_union(TfidfVectorizer(analyzer="char_wb", ngram_range=(2, 5), min_df=2, sublinear_tf=True),
                   TfidfVectorizer(ngram_range=(1, 2), min_df=2, sublinear_tf=True)),
        LogisticRegression(C=4, max_iter=3000, class_weight="balanced"))


def main():
    rows = load()
    X = [text(r) for r in rows]
    y = np.array([r["code"] != "n" for r in rows])
    files = np.array([bool(r["files"]) for r in rows])
    kw = np.array([r["keyword_filter"] for r in rows]) | files
    p = np.zeros(len(rows))
    for tr, te in StratifiedKFold(5, shuffle=True, random_state=0).split(X, y):
        p[te] = model().fit([X[i] for i in tr], y[tr]).predict_proba([X[i] for i in te])[:, 1]
    t = next(t for t in np.linspace(0.99, 0, 500) if ((p >= t) | files)[y].mean() >= TARGET_RECALL)
    send = (p >= t) | files
    print(f"сообщений {len(rows)}, полезных {y.mean():.0%}")
    print(f"ключевые слова: ловят {kw[y].mean():.0%} полезных, в модель идёт {kw.mean():.0%}")
    print(f"классификатор:  ловит {send[y].mean():.0%} полезных, в модель идёт {send.mean():.0%} (порог {t:.2f})")
    with open(CHATS / "filter_model.pkl", "wb") as f:
        pickle.dump({"model": model().fit(X, y), "threshold": float(t)}, f)
    print("сохранил chats/filter_model.pkl")


if __name__ == "__main__":
    main()
