//! The single dispatch table for every academic source the engine can query.
//!
//! Before this existed, `AcademicEngine::search_candidates` held one hand-written
//! future per source and joined them through ten nested tuples. Adding a source
//! meant editing six places, and because every slot had the same type
//! (`Option<Result<Vec<Paper>, String>>`) a mis-ordered tuple silently attributed
//! one source's results to another without the compiler noticing. Here a source
//! is one row: an id that matches the catalog, the name shown to the user, an
//! optional shared rate-limit gate, and the call itself.

use crate::engine::AcademicEngine;
use crate::models::{Paper, SourceCredentials};
use crate::sources;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

type DriverFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Paper>, String>> + Send + 'a>>;

/// Everything a driver may need, so each row stays a single call expression.
pub struct SearchCtx<'a> {
    pub engine: &'a AcademicEngine,
    pub client: &'a reqwest::Client,
    /// The raw user query. Drivers call [`SearchCtx::adapted`] rather than using
    /// it directly, so each source gets the syntax it understands.
    pub query: &'a str,
    pub limit: usize,
    pub year_min: Option<u32>,
    pub year_max: Option<u32>,
    pub creds: &'a SourceCredentials,
    /// True when the selection is Vietnam-only, which widens that source's page.
    pub only_vn: bool,
    /// Wall-clock budget for the whole fan-out. Drivers that set their own,
    /// longer HTTP timeout (the server-rendered OJS portals) cap it against
    /// this so no single source can outlive the search it belongs to.
    pub budget: Duration,
}

impl SearchCtx<'_> {
    pub fn adapted(&self, id: &str) -> String {
        crate::query::adapt(id, self.query)
    }

    /// A Vietnam-only search has no other source to fill the page, so ask for
    /// more from the one that is left.
    pub fn vietnam_limit(&self) -> usize {
        if self.only_vn {
            self.limit.max(20)
        } else {
            self.limit
        }
    }
}

pub struct SourceDriver {
    /// Matches the `id` in `searchCatalog.json`; also the query-adapter id.
    pub id: &'static str,
    pub name: &'static str,
    /// `(gate group, minimum spacing in ms)`. Sources sharing one upstream rate
    /// limit — every NCBI E-utilities endpoint, or OpenAlex and its Vietnam
    /// view — name the same group so they queue behind a single gate.
    pub gate: Option<(&'static str, u64)>,
    pub fetch: for<'a> fn(&'a SearchCtx<'a>) -> DriverFuture<'a>,
}

impl SourceDriver {
    /// Waits for this source's rate-limit gate, then runs the fetch.
    pub async fn run(&self, ctx: &SearchCtx<'_>) -> Result<Vec<Paper>, String> {
        if let Some((group, interval_ms)) = self.gate {
            let gate = gate_for(group);
            let mut next = gate.lock().await;
            if let Some(ready) = *next {
                tokio::time::sleep_until(ready).await;
            }
            *next = Some(tokio::time::Instant::now() + Duration::from_millis(interval_ms));
        }
        (self.fetch)(ctx).await
    }
}

type Gate = Arc<Mutex<Option<tokio::time::Instant>>>;

fn gate_for(group: &'static str) -> Gate {
    static GATES: OnceLock<std::sync::Mutex<HashMap<&'static str, Gate>>> = OnceLock::new();
    let gates = GATES.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut gates = gates.lock().unwrap_or_else(|p| p.into_inner());
    gates.entry(group).or_default().clone()
}

/// Every source the engine can query, in the order their statuses are reported.
///
/// `metasearch`/`searxng` is deliberately absent: it is one status slot whose id
/// and name depend on whether an external SearXNG is configured, so the engine
/// resolves it separately.
pub fn drivers() -> &'static [SourceDriver] {
    static DRIVERS: OnceLock<Vec<SourceDriver>> = OnceLock::new();
    DRIVERS.get_or_init(|| {
        vec![
        SourceDriver {
            id: "openalex",
            name: "OpenAlex",
            gate: Some(("openalex", 200)),
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_openalex(&ctx.adapted("openalex"), ctx.limit, ctx.year_min, ctx.year_max, ctx.creds).await }),
        },
        SourceDriver {
            id: "pubmed",
            name: "PubMed",
            gate: Some(("ncbi_eutils", 400)),
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_pubmed(&ctx.adapted("pubmed"), ctx.limit, ctx.creds).await }),
        },
        SourceDriver {
            id: "arxiv",
            name: "arXiv",
            gate: None,
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_arxiv(&ctx.adapted("arxiv"), ctx.limit).await }),
        },
        SourceDriver {
            id: "crossref",
            name: "Crossref",
            gate: Some(("crossref", 300)),
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_crossref(&ctx.adapted("crossref"), ctx.limit, ctx.creds).await }),
        },
        SourceDriver {
            id: "semantic_scholar",
            name: "Semantic Scholar",
            gate: None,
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_semantic_scholar(&ctx.adapted("semantic_scholar"), ctx.limit, ctx.creds).await }),
        },
        SourceDriver {
            id: "doaj",
            name: "DOAJ",
            gate: None,
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_doaj(&ctx.adapted("doaj"), ctx.limit).await }),
        },
        SourceDriver {
            id: "zenodo",
            name: "Zenodo",
            gate: None,
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_zenodo(&ctx.adapted("zenodo"), ctx.limit).await }),
        },
        SourceDriver {
            id: "hal",
            name: "HAL",
            gate: None,
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_hal(&ctx.adapted("hal"), ctx.limit).await }),
        },
        SourceDriver {
            id: "europe_pmc",
            name: "Europe PMC",
            gate: Some(("europe_pmc", 200)),
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_europe_pmc(&ctx.adapted("europe_pmc"), ctx.limit).await }),
        },
        SourceDriver {
            id: "vietnam",
            name: "OpenAlex Vietnam",
            gate: Some(("openalex", 200)),
            fetch: |ctx| Box::pin(async move { ctx.engine.fetch_vietnam_openalex(&ctx.adapted("vietnam"), ctx.vietnam_limit(), ctx.creds).await }),
        },
        SourceDriver {
            id: "vjol",
            name: "VJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::VJOL_CONFIG, &ctx.adapted("vjol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "vast",
            name: "VAST",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::VAST_CONFIG, &ctx.adapted("vast"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "jst_hust",
            name: "JST HUST",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::JST_HUST_CONFIG, &ctx.adapted("jst_hust"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "vnu_js",
            name: "VNU Journals",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::VNU_JS_CONFIG, &ctx.adapted("vnu_js"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "medpharmres",
            name: "MedPharmRes",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::MEDPHARMRES_CONFIG, &ctx.adapted("medpharmres"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "tapchi_yhoc_tphcm",
            name: "Ho Chi Minh City Journal of Medicine",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::YHOC_TPHCM_CONFIG, &ctx.adapted("tapchi_yhoc_tphcm"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "tapchi_nghiencuuyhoc",
            name: "Journal of Medical Research (HMU)",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::NGHIENCUU_YHOC_CONFIG, &ctx.adapted("tapchi_nghiencuuyhoc"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "vista_nasati",
            name: "VISTA NASATI",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::nasati::search_nasati(ctx.client, &ctx.adapted("vista_nasati"), ctx.limit, ctx.year_min, ctx.year_max).await }),
        },
        SourceDriver {
            id: "ajol",
            name: "AJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::AJOL_CONFIG, &ctx.adapted("ajol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "thaijo",
            name: "ThaiJO",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::THAIJO_CONFIG, &ctx.adapted("thaijo"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "banglajol",
            name: "BanglaJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::BANGLAJOL_CONFIG, &ctx.adapted("banglajol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "nepjol",
            name: "NepJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::NEPJOL_CONFIG, &ctx.adapted("nepjol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "philjol",
            name: "PhilJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::PHILJOL_CONFIG, &ctx.adapted("philjol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "mongoliajol",
            name: "MongoliaJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::MONGOLIAJOL_CONFIG, &ctx.adapted("mongoliajol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "sljol",
            name: "SLJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::SLJOL_CONFIG, &ctx.adapted("sljol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "lamjol",
            name: "LAMJOL",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ojs::search_ojs(ctx.client, sources::ojs::LAMJOL_CONFIG, &ctx.adapted("lamjol"), ctx.limit, ctx.year_min, ctx.year_max, ctx.budget).await }),
        },
        SourceDriver {
            id: "clinicaltrials",
            name: "ClinicalTrials.gov",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::biomedical::search_clinicaltrials(ctx.client, &ctx.adapted("clinicaltrials"), ctx.limit).await }),
        },
        SourceDriver {
            id: "biorxiv",
            name: "bioRxiv",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::biomedical::search_biorxiv_medrxiv(ctx.client, "biorxiv", &ctx.adapted("biorxiv"), ctx.limit).await }),
        },
        SourceDriver {
            id: "medrxiv",
            name: "medRxiv",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::biomedical::search_biorxiv_medrxiv(ctx.client, "medrxiv", &ctx.adapted("medrxiv"), ctx.limit).await }),
        },
        SourceDriver {
            id: "pmc",
            name: "PubMed Central",
            gate: Some(("ncbi_eutils", 400)),
            fetch: |ctx| Box::pin(async move { sources::biomedical::search_pmc(ctx.client, &ctx.adapted("pmc"), ctx.limit).await }),
        },
        SourceDriver {
            id: "plos",
            name: "PLOS",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::biomedical::search_plos(ctx.client, &ctx.adapted("plos"), ctx.limit).await }),
        },
        SourceDriver {
            id: "dblp",
            name: "DBLP",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::cs_ai::search_dblp(ctx.client, &ctx.adapted("dblp"), ctx.limit).await }),
        },
        SourceDriver {
            id: "papers_with_code",
            name: "Papers With Code",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::cs_ai::search_huggingface(ctx.client, "papers_with_code", &ctx.adapted("papers_with_code"), ctx.limit).await }),
        },
        SourceDriver {
            id: "huggingface",
            name: "Hugging Face",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::cs_ai::search_huggingface(ctx.client, "huggingface", &ctx.adapted("huggingface"), ctx.limit).await }),
        },
        SourceDriver {
            id: "openreview",
            name: "OpenReview",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::cs_ai::search_openreview(ctx.client, &ctx.adapted("openreview"), ctx.limit).await }),
        },
        SourceDriver {
            id: "inspire_hep",
            name: "INSPIRE-HEP",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::open_repos::search_inspire_hep(ctx.client, &ctx.adapted("inspire_hep"), ctx.limit).await }),
        },
        SourceDriver {
            id: "datacite",
            name: "DataCite",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::open_repos::search_datacite(ctx.client, &ctx.adapted("datacite"), ctx.limit).await }),
        },
        SourceDriver {
            id: "econbiz",
            name: "EconBiz",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::open_repos::search_econbiz(ctx.client, &ctx.adapted("econbiz"), ctx.limit).await }),
        },
        SourceDriver {
            id: "eric",
            name: "ERIC",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::open_repos::search_eric(ctx.client, &ctx.adapted("eric"), ctx.limit).await }),
        },
        SourceDriver {
            id: "cinii",
            name: "CiNii",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_cinii(ctx.client, &ctx.adapted("cinii"), ctx.limit).await }),
        },
        SourceDriver {
            id: "dryad",
            name: "Dryad",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_dryad(ctx.client, &ctx.adapted("dryad"), ctx.limit).await }),
        },
        SourceDriver {
            id: "dataverse",
            name: "Dataverse",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_dataverse(ctx.client, &ctx.adapted("dataverse"), ctx.limit).await }),
        },
        SourceDriver {
            id: "ntrs",
            name: "NTRS",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_ntrs(ctx.client, &ctx.adapted("ntrs"), ctx.limit).await }),
        },
        SourceDriver {
            id: "world_bank",
            name: "World Bank",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_worldbank(ctx.client, &ctx.adapted("world_bank"), ctx.limit).await }),
        },
        SourceDriver {
            id: "doab",
            name: "DOAB",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_doab(ctx.client, &ctx.adapted("doab"), ctx.limit).await }),
        },
        SourceDriver {
            id: "openaire",
            name: "OpenAIRE",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_openaire(ctx.client, &ctx.adapted("openaire"), ctx.limit).await }),
        },
        SourceDriver {
            id: "cve_nvd",
            name: "NVD CVE",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_cve_nvd(ctx.client, &ctx.adapted("cve_nvd"), ctx.limit).await }),
        },
        SourceDriver {
            id: "hf_datasets",
            name: "Hugging Face Datasets",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_huggingface_datasets(ctx.client, &ctx.adapted("hf_datasets"), ctx.limit).await }),
        },
        SourceDriver {
            id: "stackexchange",
            name: "Stack Exchange",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_stackexchange(ctx.client, &ctx.adapted("stackexchange"), ctx.limit).await }),
        },
        SourceDriver {
            id: "openfda",
            name: "openFDA",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_openfda(ctx.client, &ctx.adapted("openfda"), ctx.limit).await }),
        },
        SourceDriver {
            id: "uniprot",
            name: "UniProt",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_uniprot(ctx.client, &ctx.adapted("uniprot"), ctx.limit).await }),
        },
        SourceDriver {
            id: "ncbi_geo",
            name: "NCBI GEO",
            gate: Some(("ncbi_eutils", 400)),
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_geo(ctx.client, &ctx.adapted("ncbi_geo"), ctx.limit).await }),
        },
        SourceDriver {
            id: "clinvar",
            name: "ClinVar",
            gate: Some(("ncbi_eutils", 400)),
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_clinvar(ctx.client, &ctx.adapted("clinvar"), ctx.limit).await }),
        },
        SourceDriver {
            id: "figshare",
            name: "Figshare",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_figshare(ctx.client, &ctx.adapted("figshare"), ctx.limit).await }),
        },
        SourceDriver {
            id: "sec_edgar",
            name: "SEC EDGAR",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_sec_edgar(ctx.client, &ctx.adapted("sec_edgar"), ctx.limit).await }),
        },
        SourceDriver {
            id: "cisa_kev",
            name: "CISA KEV",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::public_apis::search_cisa_kev(ctx.client, &ctx.adapted("cisa_kev"), ctx.limit).await }),
        },
        SourceDriver {
            id: "scopus",
            name: "Scopus",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_scopus(ctx.client, &ctx.adapted("scopus"), ctx.limit, ctx.creds.scopus_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "ieee",
            name: "IEEE Xplore",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_ieee(ctx.client, &ctx.adapted("ieee"), ctx.limit, ctx.creds.ieee_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "springer",
            name: "Springer Nature",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_springer(ctx.client, &ctx.adapted("springer"), ctx.limit, ctx.creds.springer_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "core",
            name: "CORE",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_core(ctx.client, &ctx.adapted("core"), ctx.limit, ctx.creds.core_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "dimensions",
            name: "Dimensions",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_dimensions(ctx.client, &ctx.adapted("dimensions"), ctx.limit, ctx.creds.dimensions_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "web_of_science",
            name: "Web of Science",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::keyed::search_web_of_science(ctx.client, &ctx.adapted("web_of_science"), ctx.limit, ctx.creds.wos_api_key.as_deref()).await }),
        },
        SourceDriver {
            id: "perplexity",
            name: "Perplexity",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ai_search::search_perplexity(ctx.client, &ctx.adapted("perplexity"), ctx.limit, ctx.creds.perplexity_api_key.as_deref(), None).await }),
        },
        SourceDriver {
            id: "consensus",
            name: "Consensus.app",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ai_search::search_consensus(ctx.client, &ctx.adapted("consensus"), ctx.limit, ctx.creds.consensus_session.as_deref()).await }),
        },
        SourceDriver {
            id: "openevidence",
            name: "OpenEvidence",
            gate: None,
            fetch: |ctx| Box::pin(async move { sources::ai_search::search_openevidence(ctx.client, &ctx.adapted("openevidence"), ctx.limit, ctx.creds.openevidence_session.as_deref()).await }),
        },
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry and the bundled catalog are two files that have to agree.
    /// A driver whose id is absent from the catalog can never be selected: the
    /// code is unreachable and nothing says so. The old hand-written fan-out
    /// had exactly this drift — seven sources had a fetch block the catalog
    /// could never switch on — and neither the compiler nor a test noticed.
    ///
    /// `available: false` is a different thing and is allowed: the driver works,
    /// the catalog has deliberately taken the source out of the product.
    #[test]
    fn every_driver_id_exists_in_the_catalog() {
        let catalog = crate::catalog::catalog();
        for driver in drivers() {
            assert!(
                catalog.sources.iter().any(|source| source.id == driver.id),
                "driver {} is not in the catalog, so nothing can ever select it",
                driver.id
            );
        }
    }

    #[test]
    fn every_available_catalog_source_has_a_driver_or_is_the_metasearch_slot() {
        let registered: std::collections::HashSet<_> =
            drivers().iter().map(|driver| driver.id).collect();
        for source in crate::catalog::catalog().sources.iter().filter(|s| s.available) {
            if matches!(source.id.as_str(), "metasearch" | "searxng") {
                continue;
            }
            assert!(
                registered.contains(source.id.as_str()),
                "catalog offers {} but no driver queries it",
                source.id
            );
        }
    }

    #[test]
    fn driver_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for driver in drivers() {
            assert!(seen.insert(driver.id), "duplicate driver id {}", driver.id);
        }
    }
}
