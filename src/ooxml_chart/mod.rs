//! OOXML 차트 (DrawingML) 파싱 및 SVG 렌더링
//!
//! HWP 파일 내 OLE 개체의 `OOXMLChartContents` 스트림 또는 HWPX `Chart/chartN.xml`은
//! Microsoft OOXML DrawingML 차트 XML로 저장된다. 이 모듈은 해당 XML을 파싱하여
//! 데이터 모델로 변환한 뒤, 네이티브 SVG 차트로 렌더링한다.
//!
//! ## 지원 범위
//! - `c:barChart` (세로/가로 막대)
//! - `c:lineChart` (꺾은선)
//! - `c:pieChart` (원형)
//! - `c:bar3DChart`·`c:pie3DChart`·`c:ofPieChart` — **2D 근사 라우팅** (C1a #1453):
//!   3D막대→평면 막대, 3D원형/ofPie→단일 원형. 입체감·보조플롯은 미표현(후속 C2).
//! - `c:scatterChart` (분산형) — `c:xVal`/`c:yVal` (x,y) 쌍, 2개 수치축,
//!   `c:scatterStyle`로 표식/직선/곡선 구분 (C1b #1660).
//! - **콤보 차트** (barChart + lineChart 혼합) — 시리즈별 타입 보존
//! - **이중 Y축** (primary + secondary) — 시리즈별 축 그룹 매핑
//!
//! ## 범위 외
//! - 3D 입체감·ofPie 보조플롯, 영역형, stock(HLC), 추세선, 애니메이션, 세밀 스타일

pub mod parser;
pub mod renderer;

/// OOXML 차트 데이터 모델
#[derive(Debug, Clone, Default)]
pub struct OoxmlChart {
    /// 주 차트 타입 (콤보인 경우 첫 번째 plotType이 들어감; 렌더러는 시리즈별 타입 우선)
    pub chart_type: OoxmlChartType,
    pub title: Option<String>,
    pub series: Vec<OoxmlSeries>,
    pub categories: Vec<String>,
    /// 시리즈 중 하나라도 보조축을 쓰면 true
    pub has_secondary_axis: bool,
    /// 막대(bar/bar3D) plot의 `c:grouping` (clustered/stacked/percentStacked).
    /// 막대 렌더러만 사용. line/pie 무관. (C1a #1453 막대 누적 보정)
    pub grouping: BarGrouping,
    /// 분산형 `c:scatterStyle` (표식/직선/곡선). scatter 렌더러만 사용. (C1b #1660)
    pub scatter_style: ScatterStyle,
}

/// 막대 차트 그룹화 방식 (`c:grouping`). line 누적은 미지원(C1d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarGrouping {
    /// 묶은(side-by-side). `clustered`/`standard` 흡수.
    #[default]
    Clustered,
    /// 누적 (시리즈를 카테고리별로 쌓음).
    Stacked,
    /// 백분율 누적 (카테고리 합을 100%로 정규화).
    PercentStacked,
}

/// 분산형 표현 방식 (`c:scatterStyle`). C1b #1660.
///
/// 한컴 분산형 5종은 이 값만으로 렌더가 결정된다(곡선 2종은 동일 `smoothMarker`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScatterStyle {
    /// 표식만 (점만, 선 없음).
    #[default]
    Marker,
    /// 직선 (선만, 표식 없음).
    Line,
    /// 직선 + 표식.
    LineMarker,
    /// 부드러운 곡선 + 표식.
    SmoothMarker,
}

impl ScatterStyle {
    /// `(선 표시, 곡선 여부, 표식 표시)`.
    pub fn flags(&self) -> (bool, bool, bool) {
        match self {
            Self::Marker => (false, false, true),
            Self::Line => (true, false, false),
            Self::LineMarker => (true, false, true),
            Self::SmoothMarker => (true, true, true),
        }
    }
}

/// 차트 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OoxmlChartType {
    /// 세로 막대 (barDir=col)
    Column,
    /// 가로 막대 (barDir=bar)
    Bar,
    /// 꺾은선
    Line,
    /// 원형
    Pie,
    /// 분산형 (x,y 산점도) (C1b #1660)
    Scatter,
    #[default]
    Unknown,
}

impl OoxmlChartType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Column => "세로 막대",
            Self::Bar => "가로 막대",
            Self::Line => "꺾은선",
            Self::Pie => "원형",
            Self::Scatter => "분산형",
            Self::Unknown => "미지원",
        }
    }
}

/// 데이터 시리즈 (막대 한 묶음 또는 선 하나)
#[derive(Debug, Clone, Default)]
pub struct OoxmlSeries {
    pub name: String,
    /// Y 값 (막대/선/원형). 분산형에서는 `c:yVal`.
    pub values: Vec<f64>,
    /// 분산형 X 값 (`c:xVal`). 분산형 전용이며 그 외 차트에서는 빈 Vec. (C1b #1660)
    pub x_values: Vec<f64>,
    /// RGB 색상 (`0xRRGGBB`), 파서가 확정 못하면 None (렌더러가 기본 팔레트 적용)
    pub color: Option<u32>,
    /// 시리즈 본인의 차트 타입 (콤보 차트에서 바/라인 구분용)
    pub series_type: OoxmlChartType,
    /// 이 시리즈가 속한 플롯의 c:axId 값 목록 (parser 내부에서 axis 분류에 사용)
    pub axis_ids: Vec<String>,
    /// 0 = 기본축(왼쪽/아래), 1 = 보조축(오른쪽/위)
    pub axis_group: u8,
    /// 숫자 포맷 코드 (예: "#,##0")
    pub format_code: Option<String>,
}

impl OoxmlChart {
    /// 파싱 입력: OOXMLChartContents 원본 바이트 (UTF-8 XML)
    pub fn parse(xml: &[u8]) -> Option<Self> {
        parser::parse_chart_xml(xml)
    }

    /// 주어진 영역에 SVG 조각으로 렌더링한다.
    /// 반환값은 `<g>...</g>` 또는 여러 요소로 구성된 SVG 문자열 조각.
    pub fn render_svg(&self, x: f64, y: f64, w: f64, h: f64) -> String {
        renderer::render_chart_svg(self, x, y, w, h)
    }

    /// 시리즈가 여러 타입을 섞어 쓰는지 (콤보 차트) 여부
    pub fn is_combo(&self) -> bool {
        let mut types: std::collections::HashSet<OoxmlChartType> = std::collections::HashSet::new();
        for s in &self.series {
            if s.series_type != OoxmlChartType::Unknown {
                types.insert(s.series_type);
            }
        }
        types.len() > 1
    }
}
