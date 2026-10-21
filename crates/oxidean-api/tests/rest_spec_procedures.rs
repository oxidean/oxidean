//! Every documented REST operation must name a real RPC procedure — catches
//! typos that would otherwise render a spec for a route that cannot work.
//! Lives in the shell crate because `PROCEDURES` is the aggregated surface
//! across `oxidean-api-core` + every domain crate (issue #162).

use std::collections::BTreeSet;

#[test]
fn every_route_procedure_is_registered() {
    let known: BTreeSet<&str> = oxidean_api::rpc::PROCEDURES.iter().copied().collect();
    for def in oxidean_api_rest::spec::route_defs()
        .iter()
        .flat_map(|d| d.iter())
    {
        assert!(
            known.contains(def.procedure),
            "{} {} → `{}` is not in rpc::PROCEDURES",
            def.method,
            def.path,
            def.procedure
        );
    }
}
