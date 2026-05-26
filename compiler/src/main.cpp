// Phase 0 smoke binary: build an LLVM module containing a single function
// `int fortytwo() { return 42; }`, JIT it via ORC v2, call it, print the
// result. Exits 0 iff the call returns 42.

#include <cstdint>
#include <iostream>
#include <memory>

#include "llvm/ExecutionEngine/Orc/LLJIT.h"
#include "llvm/ExecutionEngine/Orc/ThreadSafeModule.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/TargetSelect.h"

namespace {

std::unique_ptr<llvm::orc::ThreadSafeModule> buildFortyTwoModule() {
  auto Ctx = std::make_unique<llvm::LLVMContext>();
  auto M = std::make_unique<llvm::Module>("kardashev_phase0", *Ctx);

  llvm::IRBuilder<> Builder(*Ctx);
  auto *I32 = llvm::Type::getInt32Ty(*Ctx);
  auto *FnTy = llvm::FunctionType::get(I32, /*isVarArg=*/false);
  auto *Fn = llvm::Function::Create(
      FnTy, llvm::Function::ExternalLinkage, "fortytwo", M.get());
  auto *BB = llvm::BasicBlock::Create(*Ctx, "entry", Fn);
  Builder.SetInsertPoint(BB);
  Builder.CreateRet(llvm::ConstantInt::get(I32, 42));

  if (llvm::verifyFunction(*Fn, &llvm::errs())) {
    return nullptr;
  }

  return std::make_unique<llvm::orc::ThreadSafeModule>(
      std::move(M), std::move(Ctx));
}

} // namespace

int main() {
  llvm::InitializeNativeTarget();
  llvm::InitializeNativeTargetAsmPrinter();

  auto TSM = buildFortyTwoModule();
  if (!TSM) {
    std::cerr << "IR verification failed\n";
    return 1;
  }

  auto JITOrErr = llvm::orc::LLJITBuilder().create();
  if (!JITOrErr) {
    llvm::errs() << "LLJIT create failed: "
                 << llvm::toString(JITOrErr.takeError()) << "\n";
    return 1;
  }
  auto JIT = std::move(*JITOrErr);

  if (auto Err = JIT->addIRModule(std::move(*TSM))) {
    llvm::errs() << "addIRModule failed: "
                 << llvm::toString(std::move(Err)) << "\n";
    return 1;
  }

  auto SymOrErr = JIT->lookup("fortytwo");
  if (!SymOrErr) {
    llvm::errs() << "lookup failed: "
                 << llvm::toString(SymOrErr.takeError()) << "\n";
    return 1;
  }

  using FortyTwoFn = std::int32_t (*)();
  auto Fn = SymOrErr->toPtr<FortyTwoFn>();
  std::int32_t Result = Fn();

  std::cout << Result << std::endl;
  return Result == 42 ? 0 : 1;
}
