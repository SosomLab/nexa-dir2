# docs/wiki — GitHub Wiki 소스

이 폴더는 **Nexa Dir GitHub Wiki**(<https://github.com/SosomLab/nexa-dir2/wiki>)의 소스 미러입니다. 위키 콘텐츠를 저장소에서 버전 관리하고, 여기서 GitHub Wiki로 발행합니다.

## 구성

| 파일 | 내용 |
| --- | --- |
| `Home.md` | 위키 홈(랜딩) |
| `_Sidebar.md` / `_Footer.md` | 위키 사이드바 / 푸터 |
| `프로젝트-소개.md` · `개발-여정.md` · `설계-결정.md` | 📖 프로젝트 소개 |
| `기능-*.md` · `키보드-단축키.md` | 🧭 기능 매뉴얼 |
| `개발-*.md` | 🛠 개발자 매뉴얼(아키텍처·빌드·컴포넌트·플러그인) |
| `images/` | 스크린샷(위키 raw URL로 참조) |

## GitHub Wiki로 발행하는 법

GitHub Wiki는 별도의 git 저장소(`*.wiki.git`)이며, **최초 1회는 웹에서 첫 페이지를 만들어야** git 저장소가 초기화됩니다.

### 1단계 — 위키 초기화 (최초 1회, 웹)

<https://github.com/SosomLab/nexa-dir2/wiki> 접속 → **Create the first page** → 아무 내용이나 저장(예: "temp"). 저장 후 위키 git 저장소가 생성됩니다.

### 2단계 — 이 폴더 내용을 위키로 push

```sh
# 위키 저장소 클론
git clone git@github.com:SosomLab/nexa-dir2.wiki.git /tmp/nd2wiki

# 이 폴더 내용 복사(README.md 제외)
cp docs/wiki/*.md /tmp/nd2wiki/
rm /tmp/nd2wiki/README.md          # 위키에는 불필요
cp -r docs/wiki/images /tmp/nd2wiki/

# 커밋·push
cd /tmp/nd2wiki
git add -A
git commit -m "Nexa Dir 위키 발행"
git push
```

이후 콘텐츠 수정은 이 폴더(`docs/wiki/`)에서 하고 2단계를 반복하면 됩니다.

> ⚠️ **발행은 한 작업의 꼬리에 붙인다.** 위키는 별도 저장소라 **main push에 딸려가지 않습니다** —
> `docs/wiki/`만 고치고 넘어가면 저장소에는 최신 문서가 있는데 사용자가 보는 위키는 옛날 그대로가
> 됩니다. 실제로 07-22 발행 이후 **11일치가 밀려**, 08-02에 쓴 클라우드 매뉴얼 222줄이 그동안
> 공개되지 않았습니다(08-02 4차에 일괄 발행). 위키 파일을 건드린 작업은 **같은 트랜잭션에서 발행**하세요.

## 참고

- 이미지는 `![](https://raw.githubusercontent.com/wiki/SosomLab/nexa-dir2/images/파일명.png)` 형식으로 참조합니다 — 위키에 push된 뒤 렌더됩니다.
- 페이지 간 링크는 `[표시 텍스트](파일명-하이픈)` 형식(GitHub Wiki가 하이픈↔공백을 자동 매핑).
