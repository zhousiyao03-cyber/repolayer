namespace go promotion.member

service MemberBenefitService {
  GetBenefitResponse GetBenefit(1: GetBenefitRequest req)
  RedeemResponse Redeem(1: RedeemRequest req)
}

struct GetBenefitRequest { 1: i64 user_id }
struct GetBenefitResponse { 1: string benefit_id }
struct RedeemRequest { 1: string benefit_id }
struct RedeemResponse { 1: bool ok }
